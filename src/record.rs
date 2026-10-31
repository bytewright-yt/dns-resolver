use std::net::{Ipv4Addr, Ipv6Addr};

use crate::{
    name::{Name, NameParseError},
    question::{QClass, QType},
    reader::{Reader, ReaderError},
    writer::Writer,
};

/// A resource record: one entry in the answer, authority or additional section.
///
/// # Wire format (RFC 1035 4.1.3)
///
/// ```text
/// +---------------------+
/// |        NAME         |  variable length, same encoding as QNAME
/// +---------------------+
/// |        TYPE         |  u16, taken from the `RData` variant
/// +---------------------+
/// |        CLASS        |  u16
/// +---------------------+
/// |        TTL          |  u32, seconds the record may be cached
/// +---------------------+
/// |       RDLENGTH      |  u16, length of RDATA in bytes
/// +---------------------+
/// |        RDATA        |  RDLENGTH bytes, format depends on TYPE
/// +---------------------+
/// ```
///
/// For `youtube.com. 300 IN A 127.0.0.1`, everything after the name is:
///
/// ```text
/// 00 01  00 01  00 00 01 2c  00 04  7f 00 00 01
/// A      IN     TTL 300      len 4  127.0.0.1
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Record {
    name: Name,
    class: QClass,
    ttl: u32,
    rdata: RData,
}

impl Record {
    /// Parse a record.
    ///
    /// RDATA is decoded according to TYPE:
    /// A must be exactly 4 bytes
    /// AAAA exactly 16
    /// any other type is kept raw as `RData::Other`.
    pub(crate) fn parse(reader: &mut Reader) -> Result<Self, RecordParseError> {
        let name = Name::parse(reader)?;
        let rtype = reader.read_u16()?;
        let class = QClass::from(reader.read_u16()?);
        let ttl = reader.read_u32()?;
        let rdlength = reader.read_u16()?;
        let rdata = RData::parse(rtype, rdlength, reader)?;

        Ok(Self {
            name,
            class,
            ttl,
            rdata,
        })
    }

    pub(crate) fn new(name: Name, class: QClass, ttl: u32, rdata: RData) -> Self {
        Self {
            name,
            class,
            ttl,
            rdata,
        }
    }

    pub(crate) fn write(&self, writer: &mut Writer) {
        self.name.write(writer);
        writer.write_u16(u16::from(self.rdata.record_type()));
        writer.write_u16(u16::from(self.class));
        writer.write_u32(self.ttl);
        writer.write_u16(self.rdata.rdlength());
        self.rdata.write(writer);
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum RecordParseError {
    #[error(transparent)]
    Name(#[from] NameParseError),
    #[error(transparent)]
    Reader(#[from] ReaderError),
    #[error("{rtype} record has {len} bytes of RDATA, expected {expected}")]
    RdataLength {
        rtype: QType,
        len: usize,
        expected: usize,
    },
}

/// The payload of a record. Its variant decides the record's TYPE, so the two
/// can't disagree.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::upper_case_acronyms)]
pub(crate) enum RData {
    /// IPv4 address: 4 bytes.
    A(Ipv4Addr),
    /// IPv6 address: 16 bytes.
    AAAA(Ipv6Addr),
    /// Any other type, kept as raw RDATA bytes.
    Other { rtype: u16, data: RawData },
}

impl RData {
    /// Decode `rdlength` bytes of RDATA for a record of type `rtype`.
    fn parse(rtype: u16, rdlength: u16, reader: &mut Reader) -> Result<Self, RecordParseError> {
        let expect = |expected: usize| {
            let len = usize::from(rdlength);
            if len == expected {
                Ok(())
            } else {
                Err(RecordParseError::RdataLength {
                    rtype: QType::from(rtype),
                    len,
                    expected,
                })
            }
        };

        Ok(match QType::from(rtype) {
            QType::A => {
                expect(4)?;
                RData::A(Ipv4Addr::from(reader.read_array()?))
            }
            QType::AAAA => {
                expect(16)?;
                RData::AAAA(Ipv6Addr::from(reader.read_array()?))
            }
            // Everything else is kept raw until we decode it
            _ => RData::Other {
                rtype,
                data: RawData {
                    bytes: reader.read_slice(usize::from(rdlength))?.to_vec(),
                    len: rdlength,
                },
            },
        })
    }

    /// The TYPE written for a record holding this data.
    pub(crate) fn record_type(&self) -> QType {
        match self {
            RData::A(_) => QType::A,
            RData::AAAA(_) => QType::AAAA,
            RData::Other { rtype, .. } => QType::from(*rtype),
        }
    }

    /// The RDLENGTH written before this data.
    pub(crate) fn rdlength(&self) -> u16 {
        match self {
            RData::A(_) => 4,
            RData::AAAA(_) => 16,
            RData::Other { data, .. } => data.len,
        }
    }

    /// Write the RDATA bytes only: no RDLENGTH.
    pub(crate) fn write(&self, writer: &mut Writer) {
        match self {
            RData::A(addr) => writer.write_slice(&addr.octets()),
            RData::AAAA(addr) => writer.write_slice(&addr.octets()),
            RData::Other { data, .. } => writer.write_slice(&data.bytes),
        }
    }
}

/// RDATA kept as raw bytes, for types we don't decode.
///
/// RDLENGTH is a `u16`, so RDATA can be at most 65535 bytes. The length is
/// checked once, when the data is created, and stored as a `u16` so it can be
/// written without a conversion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RawData {
    bytes: Vec<u8>,
    /// Always equal to `bytes.len()`.
    len: u16,
}

impl TryFrom<Vec<u8>> for RawData {
    type Error = RawDataTooLong;

    fn try_from(bytes: Vec<u8>) -> Result<Self, Self::Error> {
        let len = u16::try_from(bytes.len()).map_err(|_| RawDataTooLong(bytes.len()))?;
        Ok(Self { bytes, len })
    }
}

#[derive(Debug, thiserror::Error)]
#[error("RDATA of {0} bytes is longer than the maximum of 65535")]
pub(crate) struct RawDataTooLong(usize);

#[cfg(test)]
mod tests {
    use std::net::{Ipv4Addr, Ipv6Addr};

    use super::{RData, RawData, Record, RecordParseError};
    use crate::{
        name::Name,
        question::{QClass, QType},
        reader::Reader,
        writer::Writer,
    };

    /// `youtube.com` in wire format.
    const YOUTUBE_COM: [u8; 13] = [
        0x07, 0x79, 0x6f, 0x75, 0x74, 0x75, 0x62, 0x65, 0x03, 0x63, 0x6f, 0x6d, 0x00,
    ];

    fn raw(bytes: &[u8]) -> RawData {
        RawData::try_from(bytes.to_vec()).unwrap()
    }

    fn youtube_com() -> Name {
        Name::parse(&mut Reader::new(&YOUTUBE_COM)).unwrap()
    }

    fn write(record: &Record) -> Vec<u8> {
        let mut writer = Writer::new();
        record.write(&mut writer);
        writer.into_bytes()
    }

    #[test]
    fn a_record_on_the_wire() {
        let record = Record::new(
            youtube_com(),
            QClass::IN,
            300,
            RData::A(Ipv4Addr::LOCALHOST),
        );

        let mut expected = YOUTUBE_COM.to_vec();
        expected.extend_from_slice(&[
            0x00, 0x01, // TYPE A
            0x00, 0x01, // CLASS IN
            0x00, 0x00, 0x01, 0x2c, // TTL 300
            0x00, 0x04, // RDLENGTH 4
            0x7f, 0x00, 0x00, 0x01, // 127.0.0.1
        ]);
        assert_eq!(write(&record), expected);
    }

    #[test]
    fn aaaa_record_on_the_wire() {
        let record = Record::new(
            youtube_com(),
            QClass::IN,
            300,
            RData::AAAA(Ipv6Addr::LOCALHOST),
        );

        let mut expected = YOUTUBE_COM.to_vec();
        expected.extend_from_slice(&[
            0x00, 0x1c, // TYPE AAAA (28)
            0x00, 0x01, // CLASS IN
            0x00, 0x00, 0x01, 0x2c, // TTL 300
            0x00, 0x10, // RDLENGTH 16
        ]);
        // ::1
        expected.extend_from_slice(&[0; 15]);
        expected.push(0x01);
        assert_eq!(write(&record), expected);
    }

    #[test]
    fn other_record_writes_raw_data() {
        let record = Record::new(
            youtube_com(),
            QClass::IN,
            60,
            RData::Other {
                rtype: 99,
                data: raw(&[0xde, 0xad, 0xbe]),
            },
        );

        let mut expected = YOUTUBE_COM.to_vec();
        expected.extend_from_slice(&[
            0x00, 0x63, // TYPE 99
            0x00, 0x01, // CLASS IN
            0x00, 0x00, 0x00, 0x3c, // TTL 60
            0x00, 0x03, // RDLENGTH 3
            0xde, 0xad, 0xbe,
        ]);
        assert_eq!(write(&record), expected);
    }

    #[test]
    fn record_type_comes_from_rdata() {
        assert_eq!(RData::A(Ipv4Addr::LOCALHOST).record_type(), QType::A);
        assert_eq!(RData::AAAA(Ipv6Addr::LOCALHOST).record_type(), QType::AAAA);
        assert_eq!(
            RData::Other {
                rtype: 99,
                data: raw(&[])
            }
            .record_type(),
            QType::Other(99)
        );
    }

    fn parse(input: &[u8]) -> Result<Record, RecordParseError> {
        Record::parse(&mut Reader::new(input))
    }

    /// `youtube.com` followed by the given bytes.
    fn after_name(rest: &[u8]) -> Vec<u8> {
        let mut input = YOUTUBE_COM.to_vec();
        input.extend_from_slice(rest);
        input
    }

    #[test]
    fn parses_a_record() {
        let input = after_name(&[
            0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x01, 0x2c, 0x00, 0x04, 0x7f, 0x00, 0x00, 0x01,
        ]);

        assert_eq!(
            parse(&input).unwrap(),
            Record::new(
                youtube_com(),
                QClass::IN,
                300,
                RData::A(Ipv4Addr::LOCALHOST)
            )
        );
    }

    #[test]
    fn parses_aaaa_record() {
        let mut rest = vec![0x00, 0x1c, 0x00, 0x01, 0x00, 0x00, 0x01, 0x2c, 0x00, 0x10];
        rest.extend_from_slice(&Ipv6Addr::LOCALHOST.octets());

        assert_eq!(
            parse(&after_name(&rest)).unwrap(),
            Record::new(
                youtube_com(),
                QClass::IN,
                300,
                RData::AAAA(Ipv6Addr::LOCALHOST)
            )
        );
    }

    #[test]
    fn parses_unknown_type_as_other() {
        let input = after_name(&[
            0x00, 0x63, 0x00, 0x01, 0x00, 0x00, 0x00, 0x3c, 0x00, 0x03, 0xde, 0xad, 0xbe,
        ]);

        assert_eq!(
            parse(&input).unwrap(),
            Record::new(
                youtube_com(),
                QClass::IN,
                60,
                RData::Other {
                    rtype: 99,
                    data: raw(&[0xde, 0xad, 0xbe]),
                }
            )
        );
    }

    #[test]
    fn stops_after_rdata() {
        // A record followed by 2 more bytes: parsing must consume exactly the record
        let input = after_name(&[
            0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x01, 0x2c, 0x00, 0x04, 0x7f, 0x00, 0x00, 0x01,
            0xab, 0xcd,
        ]);
        let mut reader = Reader::new(&input);
        Record::parse(&mut reader).unwrap();

        assert_eq!(reader.read_u16().unwrap(), 0xabcd);
    }

    #[test]
    fn a_record_with_wrong_length_fails() {
        // RDLENGTH 3 is invalid for an A record
        let input = after_name(&[
            0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x01, 0x2c, 0x00, 0x03, 0x7f, 0x00, 0x00,
        ]);

        assert!(matches!(
            parse(&input),
            Err(RecordParseError::RdataLength {
                rtype: QType::A,
                len: 3,
                expected: 4,
            })
        ));
    }

    #[test]
    fn rdlength_past_end_fails() {
        // RDLENGTH says 4 bytes but only 2 follow
        let input = after_name(&[
            0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x01, 0x2c, 0x00, 0x04, 0x7f, 0x00,
        ]);

        assert!(matches!(parse(&input), Err(RecordParseError::Reader(_))));
    }

    #[test]
    fn records_round_trip() {
        for rdata in [
            RData::A(Ipv4Addr::new(142, 250, 180, 14)),
            RData::AAAA(Ipv6Addr::LOCALHOST),
            RData::Other {
                rtype: 99,
                data: raw(&[1, 2, 3]),
            },
        ] {
            let bytes = write(&Record::new(youtube_com(), QClass::IN, 300, rdata));

            assert_eq!(write(&parse(&bytes).unwrap()), bytes);
        }
    }

    #[test]
    fn undecoded_known_types_parse_as_other() {
        // Named types we don't decode yet (NS, CNAME, SOA, PTR, MX, TXT, OPT)
        // must be kept raw, not panic
        for rtype in [2u16, 5, 6, 12, 15, 16, 41] {
            let mut rest = rtype.to_be_bytes().to_vec();
            rest.extend_from_slice(&[0x00, 0x01, 0x00, 0x00, 0x01, 0x2c, 0x00, 0x02, 0xab, 0xcd]);
            let input = after_name(&rest);

            let record = parse(&input).unwrap();

            assert_eq!(
                record.rdata,
                RData::Other {
                    rtype,
                    data: raw(&[0xab, 0xcd]),
                }
            );
            assert_eq!(write(&record), input);
        }
    }

    #[test]
    fn other_reports_named_type() {
        // An NS record kept raw is still an NS record, not TYPE2
        let rdata = RData::Other {
            rtype: 2,
            data: raw(&[]),
        };

        assert_eq!(rdata.record_type(), QType::NS);
    }

    #[test]
    fn rdlength_matches_written_rdata() {
        for rdata in [
            RData::A(Ipv4Addr::LOCALHOST),
            RData::AAAA(Ipv6Addr::LOCALHOST),
            RData::Other {
                rtype: 99,
                data: raw(&[1, 2, 3]),
            },
        ] {
            let mut writer = Writer::new();
            rdata.write(&mut writer);

            assert_eq!(usize::from(rdata.rdlength()), writer.into_bytes().len());
        }
    }

    #[test]
    fn raw_data_rejects_more_than_65535_bytes() {
        assert!(RawData::try_from(vec![0; 65535]).is_ok());
        assert!(RawData::try_from(vec![0; 65536]).is_err());
    }
}
