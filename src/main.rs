mod header;
mod hexdump;
mod name;
mod question;
mod reader;
mod writer;

use std::io;

use tokio::net::UdpSocket;

use {
    header::{Header, Opcode, Rcode},
    hexdump::Hexdump,
    question::Question,
    reader::Reader,
    writer::Writer,
};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let socket = UdpSocket::bind("127.0.0.1:5353").await?;
    let mut buf = [0u8; 4096];

    'packets: loop {
        let (len, addr) = socket.recv_from(&mut buf).await?;
        let packet = &buf[..len];
        let mut reader = Reader::new(packet);

        let header = match Header::parse(&mut reader) {
            Ok(header) => header,
            Err(e) => {
                eprintln!(
                    "{addr}: failed to parse header: {e}\n{}",
                    Hexdump::new(packet)
                );
                continue;
            }
        };

        // Never reply to a response, or two servers could loop forever
        if header.is_response() {
            eprintln!("{addr}: ignoring response");
            continue;
        }

        let mut questions = Vec::with_capacity(header.qd_count as usize);
        for _ in 0..header.qd_count {
            match Question::parse(&mut reader) {
                Ok(question) => questions.push(question),
                Err(e) => {
                    eprintln!(
                        "{addr}: failed to parse question: {e}\n{}",
                        Hexdump::new(packet)
                    );
                    continue 'packets;
                }
            }
        }

        for question in &questions {
            println!("{addr}: {question}");
        }

        let rcode = match header.opcode() {
            Opcode::Query => Rcode::Refused,
            _ => Rcode::NotImplemented,
        };

        let mut writer = Writer::new();
        Header::response_to(&header, rcode).write(&mut writer);
        for question in &questions {
            question.write(&mut writer);
        }

        if let Err(e) = socket.send_to(&writer.into_bytes(), addr).await {
            eprintln!("{addr}: failed to send response: {e}");
        }
    }
}

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
}
