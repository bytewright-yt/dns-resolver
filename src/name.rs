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
            if label.0.is_empty() {
                break;
            }
            // length byte + label bytes
            total_length += 1 + label.0.len();
            if total_length > 255 {
                return Err(NameParseError::NameTooLong(total_length));
            }

            labels.push(label);
        }
        Ok(Self(labels))
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
pub(super) struct Label(Vec<u8>);

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
                let label_bytes = reader.read_slice(length as usize)?.to_vec();
                Ok(Label(label_bytes))
            }
            0b11 => Err(LabelParseError::CompressionPointer),
            prefix => Err(LabelParseError::ReservedPrefix(prefix)),
        }
    }
}

impl Eq for Label {}
/// Labels compare case-insensitively.
impl PartialEq for Label {
    fn eq(&self, other: &Self) -> bool {
        self.0.eq_ignore_ascii_case(&other.0)
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
    use crate::reader::Reader;

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

    fn name(labels: &[&[u8]]) -> Name {
        Name(labels.iter().map(|l| Label(l.to_vec())).collect())
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

        assert_eq!(parsed.0[0].0, b"YouTube");
        assert_eq!(parsed.0[1].0, b"COM");
    }

    #[test]
    fn labels_compare_case_insensitively() {
        assert_eq!(Label(b"YouTube".to_vec()), Label(b"youtube".to_vec()));
        assert_ne!(Label(b"youtube".to_vec()), Label(b"youtub".to_vec()));
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
}
