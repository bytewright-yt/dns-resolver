pub(crate) struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    pub(crate) fn read_slice(&mut self, len: usize) -> Result<&'a [u8], ReaderError> {
        let remaining = self.bytes.len() - self.pos;
        if remaining < len {
            return Err(ReaderError {
                needed: len,
                pos: self.pos,
                remaining,
            });
        }
        let slice = &self.bytes[self.pos..self.pos + len];
        self.pos += len;
        Ok(slice)
    }

    /// Read exactly `N` bytes into a fixed-size array, e.g. the 4 octets of an
    /// IPv4 address.
    pub(crate) fn read_array<const N: usize>(&mut self) -> Result<[u8; N], ReaderError> {
        let mut array = [0; N];
        array.copy_from_slice(self.read_slice(N)?);
        Ok(array)
    }

    pub(crate) fn read_u8(&mut self) -> Result<u8, ReaderError> {
        let [byte] = self.read_array()?;
        Ok(byte)
    }

    /// Read a `u16` in network byte order (big-endian).
    pub(crate) fn read_u16(&mut self) -> Result<u16, ReaderError> {
        Ok(u16::from_be_bytes(self.read_array()?))
    }

    /// Read a `u32` in network byte order (big-endian).
    pub(crate) fn read_u32(&mut self) -> Result<u32, ReaderError> {
        Ok(u32::from_be_bytes(self.read_array()?))
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Unexpected end of data: needed {needed} bytes at offset {pos}, {remaining} bytes left")]
pub(crate) struct ReaderError {
    needed: usize,
    pos: usize,
    remaining: usize,
}

#[cfg(test)]
mod tests {
    use super::Reader;

    #[test]
    fn reads_values_in_sequence() {
        let mut reader = Reader::new(&[0x3f, 0x00, b'a', b'b', b'c']);

        assert_eq!(reader.read_u16().unwrap(), 0x3f00);
        assert_eq!(reader.read_slice(3).unwrap(), b"abc");
        assert_eq!(reader.pos, 5);
    }

    #[test]
    fn read_u16_with_one_byte_left_fails() {
        let mut reader = Reader::new(&[0x01]);

        assert!(reader.read_u16().is_err());
    }

    #[test]
    fn failed_read_does_not_advance() {
        let mut reader = Reader::new(&[0x01, 0x02, 0x03, 0x04]);
        reader.read_u16().unwrap();

        assert!(reader.read_slice(3).is_err());
        assert_eq!(reader.pos, 2);
        assert_eq!(reader.read_u16().unwrap(), 0x0304);
    }

    #[test]
    fn empty_slice_at_end_succeeds() {
        let mut reader = Reader::new(&[0x01, 0x02]);
        reader.read_u16().unwrap();

        assert_eq!(reader.read_slice(0).unwrap(), b"");
    }

    #[test]
    fn huge_length_fails_without_overflow() {
        let mut reader = Reader::new(&[0x01, 0x02, 0x03]);
        reader.read_u16().unwrap();

        assert!(reader.read_slice(usize::MAX).is_err());
    }

    #[test]
    fn read_u32_is_big_endian() {
        let mut reader = Reader::new(&[0x00, 0x00, 0x01, 0x2c]);

        assert_eq!(reader.read_u32().unwrap(), 300);
        assert_eq!(reader.pos, 4);
    }

    #[test]
    fn read_u32_with_three_bytes_left_fails() {
        let mut reader = Reader::new(&[0x00, 0x00, 0x01]);

        assert!(reader.read_u32().is_err());
        assert_eq!(reader.pos, 0);
    }

    #[test]
    fn read_array_reads_fixed_size() {
        let mut reader = Reader::new(&[0x7f, 0x00, 0x00, 0x01, 0xff]);

        assert_eq!(reader.read_array::<4>().unwrap(), [0x7f, 0x00, 0x00, 0x01]);
        assert_eq!(reader.pos, 4);
    }

    #[test]
    fn read_array_past_end_fails_without_advancing() {
        let mut reader = Reader::new(&[0x7f, 0x00, 0x00]);

        assert!(reader.read_array::<4>().is_err());
        assert_eq!(reader.pos, 0);
    }
}
