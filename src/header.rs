use super::{reader::Reader, writer::Writer};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Header {
    id: u16,
    flags: Flags,
    qd_count: u16,
    an_count: u16,
    ns_count: u16,
    ar_count: u16,
}

impl Header {
    pub(crate) fn parse(reader: &mut Reader) -> Result<Self, HeaderError> {
        Ok(Self {
            id: reader.read_u16()?,
            flags: Flags::try_from(reader.read_u16()?)?,
            qd_count: reader.read_u16()?,
            an_count: reader.read_u16()?,
            ns_count: reader.read_u16()?,
            ar_count: reader.read_u16()?,
        })
    }

    /// Whether this message is a response (QR set). Servers must not reply to
    /// responses, or two servers could bounce packets between each other forever.
    pub(crate) fn is_response(&self) -> bool {
        self.flags.response
    }

    pub(crate) fn opcode(&self) -> Opcode {
        self.flags.opcode
    }

    /// Number of entries in the question section.
    pub(crate) fn qd_count(&self) -> u16 {
        self.qd_count
    }

    /// Number of records in the answer section.
    pub(crate) fn an_count(&self) -> u16 {
        self.an_count
    }

    pub(crate) fn write(&self, writer: &mut Writer) {
        writer.write_u16(self.id);
        writer.write_u16(u16::from(self.flags));
        writer.write_u16(self.qd_count);
        writer.write_u16(self.an_count);
        writer.write_u16(self.ns_count);
        writer.write_u16(self.ar_count);
    }

    /// Build the header for a response to `query`.
    ///
    /// Copies the ID (so the client can match the reply to its query), the
    /// opcode and RD; sets QR; clears every other flag; uses `rcode`. The
    /// counts are passed in by whoever built the sections, so they always
    /// match what is actually written.
    pub(crate) fn response_to(
        query: &Header,
        rcode: Rcode,
        qd_count: u16,
        an_count: u16,
    ) -> Header {
        Header {
            id: query.id,
            flags: Flags {
                response: true,
                opcode: query.flags.opcode,
                authoritative: false,
                truncated: false,
                recursion_desired: query.flags.recursion_desired,
                recursion_available: false,
                z: false,
                authenticated_data: false,
                checking_disabled: false,
                rcode,
            },
            qd_count,
            an_count,
            ns_count: 0,
            ar_count: 0,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum HeaderError {
    #[error(transparent)]
    Reader(#[from] super::reader::ReaderError),
    #[error(transparent)]
    FlagsParse(#[from] FlagsParseError),
}

// mirrors the single-bit flags on the wire
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub(crate) struct Flags {
    pub(crate) response: bool, // QR
    pub(crate) opcode: Opcode,
    pub(crate) authoritative: bool,       // AA
    pub(crate) truncated: bool,           // TC
    pub(crate) recursion_desired: bool,   // RD
    pub(crate) recursion_available: bool, // RA
    pub(crate) z: bool,                   // Z
    pub(crate) authenticated_data: bool,  // AD
    pub(crate) checking_disabled: bool,   // CD
    pub(crate) rcode: Rcode,
}

impl TryFrom<u16> for Flags {
    type Error = FlagsParseError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        let bit = |n: u16| (value & (1 << n)) != 0;
        Ok(Self {
            response: bit(15),
            opcode: Opcode::try_from(((value >> 11) & 0x0F) as u8)?,
            authoritative: bit(10),
            truncated: bit(9),
            recursion_desired: bit(8),
            recursion_available: bit(7),
            z: bit(6),
            authenticated_data: bit(5),
            checking_disabled: bit(4),
            rcode: Rcode::try_from((value & 0x0F) as u8)?,
        })
    }
}

impl From<Flags> for u16 {
    fn from(flags: Flags) -> Self {
        u16::from(flags.response) << 15
            | u16::from(flags.opcode as u8) << 11
            | u16::from(flags.authoritative) << 10
            | u16::from(flags.truncated) << 9
            | u16::from(flags.recursion_desired) << 8
            | u16::from(flags.recursion_available) << 7
            | u16::from(flags.z) << 6
            | u16::from(flags.authenticated_data) << 5
            | u16::from(flags.checking_disabled) << 4
            | u16::from(flags.rcode as u8)
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum FlagsParseError {
    #[error(transparent)]
    Opcode(#[from] OpcodeParseError),
    #[error(transparent)]
    Rcode(#[from] RcodeParseError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Opcode {
    Query = 0,
    IQuery = 1,
    Status = 2,
    Notify = 4,
    Update = 5,
}

impl TryFrom<u8> for Opcode {
    type Error = OpcodeParseError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Opcode::Query),
            1 => Ok(Opcode::IQuery),
            2 => Ok(Opcode::Status),
            4 => Ok(Opcode::Notify),
            5 => Ok(Opcode::Update),
            _ => Err(OpcodeParseError(value)),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Invalid opcode value: {0}")]
pub(crate) struct OpcodeParseError(u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Rcode {
    NoError = 0,
    FormatError = 1,
    ServerFailure = 2,
    NameError = 3,
    NotImplemented = 4,
    Refused = 5,
}

impl TryFrom<u8> for Rcode {
    type Error = RcodeParseError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Rcode::NoError),
            1 => Ok(Rcode::FormatError),
            2 => Ok(Rcode::ServerFailure),
            3 => Ok(Rcode::NameError),
            4 => Ok(Rcode::NotImplemented),
            5 => Ok(Rcode::Refused),
            _ => Err(RcodeParseError(value)),
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Invalid rcode value: {0}")]
pub(crate) struct RcodeParseError(u8);

#[cfg(test)]
mod tests {
    use super::{Flags, Header, Opcode, Rcode, Reader, Writer};

    #[test]
    fn parse_dig_query_header() {
        // Header of a query captured from `dig @127.0.0.1 -p 5353 youtube.com +tries=1 +timeout=1 +noedns`
        let input = [
            0x3f, 0x00, 0x01, 0x20, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];
        let expected = Header {
            id: 0x3f00,
            flags: Flags {
                response: false,
                opcode: Opcode::Query,
                authoritative: false,
                truncated: false,
                recursion_desired: true,
                recursion_available: false,
                z: false,
                authenticated_data: true,
                checking_disabled: false,
                rcode: Rcode::NoError,
            },
            qd_count: 1,
            an_count: 0,
            ns_count: 0,
            ar_count: 0,
        };
        assert_eq!(Header::parse(&mut Reader::new(&input)).unwrap(), expected);
    }

    #[test]
    fn parse_rejects_short_header() {
        assert!(Header::parse(&mut Reader::new(&[0x3f, 0x00, 0x01])).is_err());
    }

    #[test]
    fn flags_round_trip() {
        // query, response, NXDOMAIN response, all clear
        for bits in [0x0120, 0x8180, 0x8183, 0x0000] {
            assert_eq!(u16::from(Flags::try_from(bits).unwrap()), bits);
        }
    }

    fn round_trip(input: &[u8]) -> Vec<u8> {
        let header = Header::parse(&mut Reader::new(input)).unwrap();
        let mut writer = Writer::new();
        header.write(&mut writer);
        writer.into_bytes()
    }

    #[test]
    fn dig_query_header_round_trips() {
        let input = [
            0x3f, 0x00, 0x01, 0x20, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ];

        assert_eq!(round_trip(&input), input);
    }

    #[test]
    fn response_header_round_trips() {
        // NXDOMAIN response: QR, RD, RA set, RCODE 3, every count different
        let input = [
            0xbe, 0xef, 0x81, 0x83, 0x00, 0x01, 0x00, 0x02, 0x00, 0x03, 0x00, 0x04,
        ];

        assert_eq!(round_trip(&input), input);
    }

    /// Flags with only the given fields set, for building expected values.
    fn flags(response: bool, opcode: Opcode, recursion_desired: bool, rcode: Rcode) -> Flags {
        Flags {
            response,
            opcode,
            authoritative: false,
            truncated: false,
            recursion_desired,
            recursion_available: false,
            z: false,
            authenticated_data: false,
            checking_disabled: false,
            rcode,
        }
    }

    fn query_header(input: &[u8]) -> Header {
        Header::parse(&mut Reader::new(input)).unwrap()
    }

    #[test]
    fn response_to_dig_query() {
        // Header of `dig @127.0.0.1 -p 5353 youtube.com +tries=1 +timeout=1 +noedns`: RD and AD set
        let query = query_header(&[
            0x3f, 0x00, 0x01, 0x20, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ]);

        let response = Header::response_to(&query, Rcode::Refused, 1, 0);

        assert_eq!(
            response,
            Header {
                id: 0x3f00,
                flags: flags(true, Opcode::Query, true, Rcode::Refused),
                qd_count: 1,
                an_count: 0,
                ns_count: 0,
                ar_count: 0,
            }
        );
    }

    #[test]
    fn response_to_dig_query_on_the_wire() {
        let query = query_header(&[
            0x3f, 0x00, 0x01, 0x20, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ]);
        let mut writer = Writer::new();
        Header::response_to(&query, Rcode::Refused, 1, 0).write(&mut writer);

        // Flags 0x8105: QR (0x8000) | RD (0x0100) | RCODE 5 (REFUSED)
        assert_eq!(
            writer.into_bytes(),
            [
                0x3f, 0x00, 0x81, 0x05, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00
            ]
        );
    }

    #[test]
    fn response_copies_rd_when_clear() {
        // `dig +norecurse`: RD not set, so the response mustn't set it either
        let query = query_header(&[
            0x12, 0x34, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ]);

        let response = Header::response_to(&query, Rcode::Refused, 1, 0);

        assert!(!response.flags.recursion_desired);
    }

    #[test]
    fn response_copies_opcode() {
        // STATUS query (opcode 2): answered with NOTIMP, opcode echoed back
        let query = query_header(&[
            0x12, 0x34, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ]);

        let response = Header::response_to(&query, Rcode::NotImplemented, 0, 0);

        assert_eq!(
            response.flags,
            flags(true, Opcode::Status, false, Rcode::NotImplemented)
        );
    }

    #[test]
    fn response_clears_query_only_flags_and_counts() {
        // Every flag bit that a query could set (AA, TC, RA, Z, AD, CD), plus an
        // EDNS OPT record in the additional section and junk answer/authority counts
        let query = query_header(&[
            0x12, 0x34, 0x07, 0xf0, 0x00, 0x01, 0x00, 0x02, 0x00, 0x03, 0x00, 0x01,
        ]);

        let response = Header::response_to(&query, Rcode::Refused, 1, 0);

        assert_eq!(
            response,
            Header {
                id: 0x1234,
                flags: flags(true, Opcode::Query, true, Rcode::Refused),
                qd_count: 1,
                an_count: 0,
                ns_count: 0,
                ar_count: 0,
            }
        );
    }

    #[test]
    fn query_is_not_a_response() {
        let query = query_header(&[
            0x3f, 0x00, 0x01, 0x20, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ]);

        assert!(!query.is_response());
        assert_eq!(query.opcode(), Opcode::Query);
    }

    #[test]
    fn response_is_a_response() {
        let query = query_header(&[
            0x3f, 0x00, 0x01, 0x20, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ]);

        assert!(Header::response_to(&query, Rcode::Refused, 1, 0).is_response());
    }

    #[test]
    fn response_with_one_answer_on_the_wire() {
        let query = query_header(&[
            0x3f, 0x00, 0x01, 0x20, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        ]);
        let response = Header::response_to(&query, Rcode::NoError, 1, 1);

        let mut writer = Writer::new();
        response.write(&mut writer);

        // Flags 0x8100: QR | RD, RCODE 0 (NOERROR). QDCOUNT 1, ANCOUNT 1
        assert_eq!(
            writer.into_bytes(),
            [
                0x3f, 0x00, 0x81, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00
            ]
        );
    }
}
