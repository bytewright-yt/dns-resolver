use std::fmt::{self, Display};

pub(crate) struct Hexdump<'a> {
    bytes: &'a [u8],
}

impl<'a> Hexdump<'a> {
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { bytes }
    }
}

impl Display for Hexdump<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (row, chunk) in self.bytes.chunks(16).enumerate() {
            // Print the offset in hexadecimal format
            write!(f, "{:04x}  ", row * 16)?;

            // Print each byte in hexadecimal format
            for (i, byte) in chunk.iter().enumerate() {
                write!(f, "{byte:02x} ")?;

                // Add an extra space after 8 bytes
                if i == 7 {
                    write!(f, " ")?;
                }
            }

            // Print spaces for alignment if the chunk is less than 16 bytes
            for i in chunk.len()..16 {
                write!(f, "   ")?;
                if i == 7 {
                    write!(f, " ")?;
                }
            }

            // Print the ASCII repr
            write!(f, " |")?;
            for &byte in chunk {
                let c = if byte.is_ascii_graphic() || byte == b' ' {
                    byte as char
                } else {
                    '.'
                };
                write!(f, "{c}")?;
            }
            writeln!(f, "|")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::Hexdump;

    #[test]
    fn empty_input_produces_no_output() {
        assert_eq!(Hexdump::new(&[][..]).to_string(), "");
    }

    #[test]
    fn dig_query_for_youtube_com() {
        // A query captured from `dig @127.0.0.1 -p 5353 youtube.com +tries=1 +timeout=1 +noedns`
        let input = [
            0x3f, 0x00, 0x01, 0x20, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07, 0x79,
            0x6f, 0x75, 0x74, 0x75, 0x62, 0x65, 0x03, 0x63, 0x6f, 0x6d, 0x00, 0x00, 0x01, 0x00,
            0x01,
        ];
        let expected = concat!(
            "0000  3f 00 01 20 00 01 00 00  00 00 00 00 07 79 6f 75  |?.. .........you|\n",
            "0010  74 75 62 65 03 63 6f 6d  00 00 01 00 01           |tube.com.....|\n",
        );
        assert_eq!(Hexdump::new(&input[..]).to_string(), expected);
    }
}
