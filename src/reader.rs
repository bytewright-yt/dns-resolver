pub(super) struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    pub(super) fn read_slice(&mut self, len: usize) -> Result<&'a [u8], ReaderError> {
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

    pub(super) fn read_u16(&mut self) -> Result<u16, ReaderError> {
        let slice = self.read_slice(2)?;
        Ok(u16::from_be_bytes([slice[0], slice[1]]))
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Unexpected end of data: needed {needed} bytes at offset {pos}, {remaining} bytes left")]
pub(super) struct ReaderError {
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
}
