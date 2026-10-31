/// Builds a DNS message byte by byte. The counterpart to [`Reader`](crate::reader::Reader).
///
/// Writing to a `Vec` can't fail.
pub(super) struct Writer {
    bytes: Vec<u8>,
}

impl Writer {
    pub(super) fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    pub(super) fn write_u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    /// Writes `value` in network byte order (big-endian).
    pub(super) fn write_u16(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }

    pub(super) fn write_slice(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    pub(super) fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::Writer;

    #[test]
    fn writes_values_in_sequence() {
        let mut writer = Writer::new();
        writer.write_u8(0x07);
        writer.write_u16(0x3f00);
        writer.write_slice(b"abc");

        assert_eq!(writer.into_bytes(), [0x07, 0x3f, 0x00, b'a', b'b', b'c']);
    }

    #[test]
    fn write_u16_is_big_endian() {
        let mut writer = Writer::new();
        writer.write_u16(0x0102);

        assert_eq!(writer.into_bytes(), [0x01, 0x02]);
    }

    #[test]
    fn new_writer_is_empty() {
        assert_eq!(Writer::new().into_bytes(), []);
    }
}
