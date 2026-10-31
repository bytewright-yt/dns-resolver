use super::reader::Reader;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Header {
    id: u16,
    flags: Flags,
    pub(super) qd_count: u16,
    an_count: u16,
    ns_count: u16,
    ar_count: u16,
}

impl Header {
    pub fn parse(reader: &mut Reader) -> Result<Self, HeaderError> {
        Ok(Self {
            id: reader.read_u16()?,
            flags: Flags::try_from(reader.read_u16()?)?,
            qd_count: reader.read_u16()?,
            an_count: reader.read_u16()?,
            ns_count: reader.read_u16()?,
            ar_count: reader.read_u16()?,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub(super) enum HeaderError {
    #[error(transparent)]
    Reader(#[from] super::reader::ReaderError),
    #[error(transparent)]
    FlagsParse(#[from] FlagsParseError),
}

// mirrors the single-bit flags on the wire
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub(super) struct Flags {
    response: bool, // QR
    opcode: Opcode,
    authoritative: bool,       // AA
    truncated: bool,           // TC
    recursion_desired: bool,   // RD
    recursion_available: bool, // RA
    z: bool,                   // Z
    authenticated_data: bool,  // AD
    checking_disabled: bool,   // CD
    rcode: Rcode,
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
pub(super) enum FlagsParseError {
    #[error(transparent)]
    Opcode(#[from] OpcodeParseError),
    #[error(transparent)]
    Rcode(#[from] RcodeParseError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Opcode {
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
pub(super) struct OpcodeParseError(u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Rcode {
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
pub(super) struct RcodeParseError(u8);

#[cfg(test)]
mod tests {
    use super::{Flags, Header, Opcode, Rcode, Reader};

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
}
