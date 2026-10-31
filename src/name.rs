use std::fmt;

use crate::writer::Writer;

/// A domain name, e.g. "www.youtube.com", stored as its list of labels.
///
/// Names compare case-insensitively, but the wire format preserves case, so
/// the labels are stored exactly as received.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Name(Vec<Label>);

impl Name {
    /// Parse a domain name from the reader.
    ///
    /// # Wire format (RFC 1035 3.1)
    ///
    /// The `.` only exists in the text form. On the wire a name is a sequence of
    /// labels, each a length byte followed by that many bytes, ending with a
    /// zero-length label (a single `00` byte, the root):
    ///
    /// ```text
    /// 07 79 6f 75 74 75 62 65  03 63 6f 6d  00
    /// 7  y  o  u  t  u  b  e   3  c  o  m   end
    /// ```
    ///
    /// The whole name, including every length byte and the final `00`, is
    /// limited to 255 bytes on the wire (RFC 1035 2.3.4).
    pub(super) fn parse(reader: &mut super::reader::Reader) -> Result<Self, NameParseError> {
        let mut labels = Vec::new();
        // Starts at 1 for the terminating 0 byte
        let mut total_length = 1;
        loop {
            let label = Label::parse(reader)?;
            if label.is_empty() {
                break;
            }
            // length byte + label bytes
            total_length += 1 + label.len();
            if total_length > 255 {
                return Err(NameParseError::NameTooLong(total_length));
            }

            labels.push(label);
        }
        Ok(Self(labels))
    }

    pub(super) fn write(&self, writer: &mut Writer) {
        for label in &self.0 {
            label.write(writer);
        }
        // Terminating 0-length label
        writer.write_u8(0);
    }
}

impl fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0.is_empty() {
            write!(f, ".")?;
        }
        for label in &self.0 {
            write!(f, "{label}.")?;
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub(super) enum NameParseError {
    #[error(transparent)]
    Reader(#[from] super::reader::ReaderError),
    #[error(transparent)]
    Label(#[from] LabelParseError),
    #[error("Domain name exceeds maximum length of 255 bytes on the wire: {0}")]
    NameTooLong(usize),
}

/// A single label in a domain name, e.g. "www.youtube.com" is 3 labels "www", "youtube" and "com".
///
/// Labels are raw bytes, not guaranteed to be ASCII or UTF-8. An empty label
/// is the root, which marks the end of a name.
#[derive(Debug, Clone)]
pub(super) struct Label {
    bytes: Vec<u8>,
    /// Always equal to `bytes.len()`, and at most 63. Stored as a `u8` so it
    /// can be written as the length byte without a conversion.
    len: u8,
}

impl Label {
    /// Parse a single label from the reader.
    ///
    /// # Wire format (RFC 1035 4.1.4)
    ///
    /// Each label starts with one byte. Its top 2 bits say what kind of entry
    /// it is, and the bottom 6 bits are its value:
    ///
    /// ```text
    ///   7   6   5   4   3   2   1   0
    /// +---+---+---+---+---+---+---+---+
    /// | type  |        length         |
    /// +---+---+---+---+---+---+---+---+
    /// ```
    ///
    /// - `00`: a normal label. The 6-bit length (0-63) is followed by that
    ///   many bytes of label data. A length of 0 is the root label ending the name.
    /// - `11`: a compression pointer. The remaining 6 bits plus the next byte
    ///   form a 14-bit offset to a name earlier in the packet (not supported yet).
    /// - `01`, `10`: reserved.
    ///
    /// Because the length only has 6 bits, a label can never exceed 63 bytes:
    /// a length byte of 64 (`0x40`) has the reserved `01` prefix.
    pub(super) fn parse(reader: &mut super::reader::Reader) -> Result<Self, LabelParseError> {
        let length_byte = reader.read_u8()?;
        match length_byte >> 6 {
            0b00 => {
                let length = length_byte & 0x3f;
                let label_bytes = reader.read_slice(usize::from(length))?.to_vec();
                Ok(Label {
                    bytes: label_bytes,
                    len: length,
                })
            }
            0b11 => Err(LabelParseError::CompressionPointer),
            prefix => Err(LabelParseError::ReservedPrefix(prefix)),
        }
    }

    pub(super) fn write(&self, writer: &mut Writer) {
        writer.write_u8(self.len);
        writer.write_slice(&self.bytes);
    }

    pub(super) fn len(&self) -> usize {
        usize::from(self.len)
    }

    pub(super) fn is_empty(&self) -> bool {
        self.len == 0
    }
}

impl fmt::Display for Label {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for &byte in &self.bytes {
            match byte {
                b'.' | b'\\' => write!(f, "\\{}", byte as char)?,
                0x21..=0x7e => write!(f, "{}", byte as char)?,
                _ => write!(f, "\\{byte:03}")?,
            }
        }
        Ok(())
    }
}

impl Eq for Label {}
/// Labels compare case-insensitively.
impl PartialEq for Label {
    fn eq(&self, other: &Self) -> bool {
        self.bytes.eq_ignore_ascii_case(&other.bytes)
    }
}

#[derive(Debug, thiserror::Error)]
pub(super) enum LabelParseError {
    #[error(transparent)]
    Reader(#[from] super::reader::ReaderError),
    #[error("Compression pointers are not supported")]
    CompressionPointer,
    #[error("Reserved label length prefix {0:02b} is not supported")]
    ReservedPrefix(u8),
}

#[cfg(test)]
mod tests {
    use super::{Label, Name};
    use crate::{reader::Reader, writer::Writer};

    /// Encode labels in wire format: length byte, label bytes, ..., terminating 0.
    fn wire(labels: &[&[u8]]) -> Vec<u8> {
        let mut out = Vec::new();
        for label in labels {
            out.push(u8::try_from(label.len()).unwrap());
            out.extend_from_slice(label);
        }
        out.push(0);
        out
    }

    fn label(bytes: &[u8]) -> Label {
        Label {
            bytes: bytes.to_vec(),
            len: u8::try_from(bytes.len()).unwrap(),
        }
    }

    fn name(labels: &[&[u8]]) -> Name {
        Name(labels.iter().map(|l| label(l)).collect())
    }

    #[test]
    fn parses_youtube_com_from_dig_query() {
        // Question section of `dig @127.0.0.1 -p 5353 youtube.com +tries=1 +timeout=1 +noedns`
        let input = [
            0x07, 0x79, 0x6f, 0x75, 0x74, 0x75, 0x62, 0x65, 0x03, 0x63, 0x6f, 0x6d, 0x00, 0x00,
            0x01, 0x00, 0x01,
        ];
        let mut reader = Reader::new(&input);

        assert_eq!(
            Name::parse(&mut reader).unwrap(),
            name(&[b"youtube", b"com"])
        );
        // Stops after the terminating 0, leaving QTYPE and QCLASS unread
        assert_eq!(reader.read_u16().unwrap(), 1);
    }

    #[test]
    fn preserves_case_when_parsing() {
        // The response must echo the question back exactly, so case must survive parsing.
        // Compare raw bytes, since `==` on labels ignores case.
        let input = wire(&[b"YouTube", b"COM"]);
        let parsed = Name::parse(&mut Reader::new(&input)).unwrap();

        assert_eq!(parsed.0[0].bytes, b"YouTube");
        assert_eq!(parsed.0[1].bytes, b"COM");
    }

    #[test]
    fn labels_compare_case_insensitively() {
        assert_eq!(label(b"YouTube"), label(b"youtube"));
        assert_ne!(label(b"youtube"), label(b"youtub"));
    }

    #[test]
    fn names_compare_case_insensitively() {
        assert_eq!(
            name(&[b"WWW", b"YouTube", b"com"]),
            name(&[b"www", b"youtube", b"COM"])
        );
        assert_ne!(
            name(&[b"youtube", b"com"]),
            name(&[b"www", b"youtube", b"com"])
        );
    }

    #[test]
    fn parses_root_name() {
        let mut reader = Reader::new(&[0x00]);

        assert_eq!(Name::parse(&mut reader).unwrap(), name(&[]));
    }

    #[test]
    fn truncated_label_fails() {
        let mut reader = Reader::new(&[0x07, b'y', b'o']);

        assert!(Name::parse(&mut reader).is_err());
    }

    #[test]
    fn missing_terminator_fails() {
        let mut reader = Reader::new(&[0x03, b'c', b'o', b'm']);

        assert!(Name::parse(&mut reader).is_err());
    }

    #[test]
    fn label_of_63_bytes_is_allowed() {
        let label = [b'a'; 63];
        let input = wire(&[&label]);

        assert_eq!(
            Name::parse(&mut Reader::new(&input)).unwrap(),
            name(&[&label])
        );
    }

    #[test]
    fn label_of_64_bytes_fails() {
        // 0x40 also has the reserved 01 prefix, so this fails either way
        let label = [b'a'; 64];
        let input = wire(&[&label]);

        assert!(Name::parse(&mut Reader::new(&input)).is_err());
    }

    #[test]
    fn compression_pointer_fails() {
        // Pointer to offset 12, where the question name starts in a full packet
        let mut reader = Reader::new(&[0xc0, 0x0c]);

        assert!(Name::parse(&mut reader).is_err());
    }

    #[test]
    fn name_of_255_bytes_is_allowed() {
        // 3 * (1 + 63) + (1 + 61) + 1 = 255 bytes on the wire
        let long = [b'a'; 63];
        let short = [b'b'; 61];
        let input = wire(&[&long, &long, &long, &short]);
        assert_eq!(input.len(), 255);

        assert_eq!(
            Name::parse(&mut Reader::new(&input)).unwrap(),
            name(&[&long, &long, &long, &short])
        );
    }

    #[test]
    fn name_of_256_bytes_fails() {
        // 3 * (1 + 63) + (1 + 62) + 1 = 256 bytes on the wire
        let long = [b'a'; 63];
        let short = [b'b'; 62];
        let input = wire(&[&long, &long, &long, &short]);
        assert_eq!(input.len(), 256);

        assert!(Name::parse(&mut Reader::new(&input)).is_err());
    }

    #[test]
    fn name_of_257_bytes_fails() {
        // 4 * (1 + 63) + 1 = 257 bytes on the wire
        let long = [b'a'; 63];
        let input = wire(&[&long, &long, &long, &long]);
        assert_eq!(input.len(), 257);

        assert!(Name::parse(&mut Reader::new(&input)).is_err());
    }

    #[test]
    fn displays_fully_qualified_name() {
        assert_eq!(name(&[b"youtube", b"com"]).to_string(), "youtube.com.");
    }

    #[test]
    fn displays_root_as_dot() {
        assert_eq!(name(&[]).to_string(), ".");
    }

    #[test]
    fn display_preserves_case() {
        assert_eq!(name(&[b"YouTube", b"COM"]).to_string(), "YouTube.COM.");
    }

    #[test]
    fn display_escapes_dot_and_backslash() {
        // One label "a.b" must not look like two labels "a" and "b"
        assert_eq!(name(&[b"a.b", b"com"]).to_string(), r"a\.b.com.");
        assert_eq!(name(&[b"a\\b", b"com"]).to_string(), r"a\\b.com.");
    }

    #[test]
    fn display_escapes_unprintable_bytes_as_decimal() {
        // Like dig: \DDD with 3 decimal digits, including space
        assert_eq!(name(&[b"a\nb", b"com"]).to_string(), r"a\010b.com.");
        assert_eq!(name(&[b"a b"]).to_string(), r"a\032b.");
        assert_eq!(name(&[b"\x00\xff"]).to_string(), r"\000\255.");
    }

    fn round_trip(input: &[u8]) -> Vec<u8> {
        let name = Name::parse(&mut Reader::new(input)).unwrap();
        let mut writer = Writer::new();
        name.write(&mut writer);
        writer.into_bytes()
    }

    #[test]
    fn name_round_trips_preserving_case() {
        // The response must echo the question name byte for byte
        let input = wire(&[b"YouTube", b"COM"]);

        assert_eq!(round_trip(&input), input);
    }

    #[test]
    fn root_name_round_trips() {
        assert_eq!(round_trip(&[0x00]), [0x00]);
    }

    #[test]
    fn longest_name_round_trips() {
        let long = [b'a'; 63];
        let short = [b'b'; 61];
        let input = wire(&[&long, &long, &long, &short]);

        assert_eq!(round_trip(&input), input);
    }

    #[test]
    fn label_writes_length_then_bytes() {
        let mut writer = Writer::new();
        label(b"com").write(&mut writer);

        assert_eq!(writer.into_bytes(), [0x03, b'c', b'o', b'm']);
    }
}
