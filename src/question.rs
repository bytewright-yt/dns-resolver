use std::fmt;

use crate::{name::Name, reader::Reader, writer::Writer};

/// A single entry in the question section: what the client is asking for.
///
/// # Wire format (RFC 1035 4.1.2)
///
/// The question section follows the 12-byte header and holds `qd_count`
/// entries (almost always 1). Each entry is a name followed by two
/// big-endian `u16`s, with no padding or alignment:
///
/// ```text
/// +---------------------+
/// |        QNAME        |  variable length, see `Name`
/// +---------------------+
/// |        QTYPE        |  2 bytes
/// +---------------------+
/// |        QCLASS       |  2 bytes
/// +---------------------+
/// ```
///
/// For `dig youtube.com`:
///
/// ```text
/// 07 79 6f 75 74 75 62 65 03 63 6f 6d 00  00 01  00 01
/// youtube.com                             A      IN
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Question {
    name: Name,
    qtype: QType,
    qclass: QClass,
}

impl Question {
    pub(super) fn parse(reader: &mut Reader) -> Result<Self, QuestionParseError> {
        Ok(Self {
            name: Name::parse(reader)?,
            qtype: QType::from(reader.read_u16()?),
            qclass: QClass::from(reader.read_u16()?),
        })
    }

    pub(super) fn write(&self, writer: &mut Writer) {
        self.name.write(writer);
        writer.write_u16(u16::from(self.qtype));
        writer.write_u16(u16::from(self.qclass));
    }
}

/// Display the question in the same order as `dig` does: name, class, type.
impl fmt::Display for Question {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} {}", self.name, self.qclass, self.qtype)
    }
}

#[derive(Debug, thiserror::Error)]
pub(super) enum QuestionParseError {
    #[error(transparent)]
    Name(#[from] crate::name::NameParseError),
    #[error(transparent)]
    Reader(#[from] crate::reader::ReaderError),
}

/// The record type being asked for (RFC 1035 3.2.2, 3.2.3).
///
/// On the wire this is a big-endian `u16`. Only common types are named;
/// anything else is kept as `Other` so it can be echoed back unchanged.
///
/// Variant names match the RFC mnemonics as shown by `dig`.
#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum QType {
    A,
    NS,
    CNAME,
    SOA,
    PTR,
    MX,
    TXT,
    AAAA,
    // EDNS
    Opt,
    ANY,
    // Catch any other types that are not explicitly handled
    Other(u16),
}

impl fmt::Display for QType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QType::A => write!(f, "A"),
            QType::NS => write!(f, "NS"),
            QType::CNAME => write!(f, "CNAME"),
            QType::SOA => write!(f, "SOA"),
            QType::PTR => write!(f, "PTR"),
            QType::MX => write!(f, "MX"),
            QType::TXT => write!(f, "TXT"),
            QType::AAAA => write!(f, "AAAA"),
            QType::Opt => write!(f, "OPT"),
            QType::ANY => write!(f, "ANY"),
            QType::Other(other) => write!(f, "TYPE{other}"),
        }
    }
}

impl From<u16> for QType {
    fn from(value: u16) -> Self {
        match value {
            1 => QType::A,
            2 => QType::NS,
            5 => QType::CNAME,
            6 => QType::SOA,
            12 => QType::PTR,
            15 => QType::MX,
            16 => QType::TXT,
            28 => QType::AAAA,
            41 => QType::Opt,
            255 => QType::ANY,
            other => QType::Other(other),
        }
    }
}

impl From<QType> for u16 {
    fn from(qtype: QType) -> Self {
        match qtype {
            QType::A => 1,
            QType::NS => 2,
            QType::CNAME => 5,
            QType::SOA => 6,
            QType::PTR => 12,
            QType::MX => 15,
            QType::TXT => 16,
            QType::AAAA => 28,
            QType::Opt => 41,
            QType::ANY => 255,
            QType::Other(other) => other,
        }
    }
}

/// The class of the record being asked for (RFC 1035 3.2.4, 3.2.5).
///
/// On the wire this is a big-endian `u16`. In practice it is almost always
/// `IN` (1, the Internet). `CH` (3, Chaos) is still seen for queries like
/// `dig CH TXT version.bind`, and is kept as `Other(3)`.
#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum QClass {
    IN,
    ANY,
    Other(u16),
}

impl fmt::Display for QClass {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QClass::IN => write!(f, "IN"),
            QClass::ANY => write!(f, "ANY"),
            QClass::Other(other) => write!(f, "CLASS{other}"),
        }
    }
}

impl From<u16> for QClass {
    fn from(value: u16) -> Self {
        match value {
            1 => QClass::IN,
            255 => QClass::ANY,
            other => QClass::Other(other),
        }
    }
}

impl From<QClass> for u16 {
    fn from(qclass: QClass) -> Self {
        match qclass {
            QClass::IN => 1,
            QClass::ANY => 255,
            QClass::Other(other) => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{QClass, QType, Question};
    use crate::{header::Header, name::Name, reader::Reader, writer::Writer};

    #[test]
    fn parses_question_from_dig_query() {
        // Question section of `dig @127.0.0.1 -p 5353 youtube.com +tries=1 +timeout=1 +noedns`
        let input = [
            0x07, 0x79, 0x6f, 0x75, 0x74, 0x75, 0x62, 0x65, 0x03, 0x63, 0x6f, 0x6d, 0x00, 0x00,
            0x01, 0x00, 0x01,
        ];
        let mut reader = Reader::new(&input);
        let question = Question::parse(&mut reader).unwrap();

        let name = Name::parse(&mut Reader::new(&input[..13])).unwrap();
        assert_eq!(
            question,
            Question {
                name,
                qtype: QType::A,
                qclass: QClass::IN,
            }
        );
        // Consumes the whole question and nothing more
        assert!(reader.read_u16().is_err());
    }

    #[test]
    fn missing_qclass_fails() {
        // Name and QTYPE, but no QCLASS
        let input = [0x03, b'c', b'o', b'm', 0x00, 0x00, 0x01];

        assert!(Question::parse(&mut Reader::new(&input)).is_err());
    }

    #[test]
    fn known_qtypes_round_trip() {
        for value in [1, 2, 5, 6, 12, 15, 16, 28, 41, 255] {
            let qtype = QType::from(value);
            assert!(!matches!(qtype, QType::Other(_)), "{value} parsed as Other");
            assert_eq!(u16::from(qtype), value);
        }
    }

    #[test]
    fn unknown_qtype_round_trips_as_other() {
        // 65 is HTTPS, which isn't named (yet)
        assert_eq!(QType::from(65), QType::Other(65));
        assert_eq!(u16::from(QType::Other(65)), 65);
    }

    #[test]
    fn qclass_round_trips() {
        for value in [1, 255] {
            let qclass = QClass::from(value);
            assert!(
                !matches!(qclass, QClass::Other(_)),
                "{value} parsed as Other"
            );
            assert_eq!(u16::from(qclass), value);
        }
        // CH (Chaos) isn't named
        assert_eq!(QClass::from(3), QClass::Other(3));
        assert_eq!(u16::from(QClass::Other(3)), 3);
    }

    /// Parse the question section of `dig youtube.com`, with QTYPE and QCLASS replaced.
    fn youtube_com_question(qtype: u16, qclass: u16) -> Question {
        let mut input = vec![
            0x07, 0x79, 0x6f, 0x75, 0x74, 0x75, 0x62, 0x65, 0x03, 0x63, 0x6f, 0x6d, 0x00,
        ];
        input.extend_from_slice(&qtype.to_be_bytes());
        input.extend_from_slice(&qclass.to_be_bytes());
        Question::parse(&mut Reader::new(&input)).unwrap()
    }

    #[test]
    fn displays_like_dig() {
        // dig prints name, class, then type
        assert_eq!(youtube_com_question(1, 1).to_string(), "youtube.com. IN A");
        assert_eq!(
            youtube_com_question(28, 1).to_string(),
            "youtube.com. IN AAAA"
        );
    }

    #[test]
    fn displays_unknown_type_and_class_in_rfc_3597_form() {
        // HTTPS (65) in the CH (3) class: neither is named
        assert_eq!(
            youtube_com_question(65, 3).to_string(),
            "youtube.com. CLASS3 TYPE65"
        );
    }

    #[test]
    fn dig_question_round_trips() {
        let input = [
            0x07, 0x79, 0x6f, 0x75, 0x74, 0x75, 0x62, 0x65, 0x03, 0x63, 0x6f, 0x6d, 0x00, 0x00,
            0x01, 0x00, 0x01,
        ];
        let question = Question::parse(&mut Reader::new(&input)).unwrap();
        let mut writer = Writer::new();
        question.write(&mut writer);

        assert_eq!(writer.into_bytes(), input);
    }

    #[test]
    fn unknown_type_and_class_round_trip() {
        // HTTPS (65) in CH (3): both come back as `Other` and must be written unchanged
        let question = youtube_com_question(65, 3);
        let mut writer = Writer::new();
        question.write(&mut writer);

        let bytes = writer.into_bytes();
        assert_eq!(bytes[bytes.len() - 4..], [0x00, 0x41, 0x00, 0x03]);
    }

    #[test]
    fn full_dig_query_round_trips() {
        // The whole 29-byte packet from `dig @127.0.0.1 -p 5353 youtube.com +tries=1 +timeout=1 +noedns`
        let input = [
            0x3f, 0x00, 0x01, 0x20, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07, 0x79,
            0x6f, 0x75, 0x74, 0x75, 0x62, 0x65, 0x03, 0x63, 0x6f, 0x6d, 0x00, 0x00, 0x01, 0x00,
            0x01,
        ];
        let mut reader = Reader::new(&input);
        let header = Header::parse(&mut reader).unwrap();
        let questions: Vec<Question> = (0..header.qd_count)
            .map(|_| Question::parse(&mut reader).unwrap())
            .collect();

        let mut writer = Writer::new();
        header.write(&mut writer);
        for question in &questions {
            question.write(&mut writer);
        }

        assert_eq!(writer.into_bytes(), input);
    }
}
