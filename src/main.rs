mod header;
mod hexdump;
mod message;
mod name;
mod question;
mod reader;
mod record;
mod writer;

use std::{io, net::SocketAddr};

use tokio::net::UdpSocket;

use {
    header::{Opcode, Rcode},
    hexdump::Hexdump,
    message::Message,
    reader::Reader,
    writer::Writer,
};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let socket = UdpSocket::bind("127.0.0.1:5300").await?;
    let mut buf = [0u8; 4096];

    loop {
        let (len, addr) = socket.recv_from(&mut buf).await?;
        let packet = &buf[..len];
        eprintln!(
            "<- packet from {addr} ({len} bytes)\n{}",
            Hexdump::new(packet)
        );

        let Some(reply) = respond(packet, addr) else {
            continue;
        };

        eprintln!(
            "-> reply to {addr} ({} bytes)\n{}",
            reply.len(),
            Hexdump::new(&reply)
        );
        if let Err(e) = socket.send_to(&reply, addr).await {
            eprintln!("{addr}: failed to send reply: {e}");
        }
    }
}

/// Build the reply to one packet, or `None` if it shouldn't get one.
fn respond(packet: &[u8], addr: SocketAddr) -> Option<Vec<u8>> {
    let query = match Message::parse(&mut Reader::new(packet)) {
        Ok(query) => query,
        Err(e) => {
            eprintln!("{addr}: failed to parse message: {e}");
            return None;
        }
    };

    // Never reply to a response, or two servers could loop forever
    if query.header().is_response() {
        eprintln!("{addr}: ignoring response");
        return None;
    }

    for question in query.questions() {
        println!("{addr}: {question}");
    }

    let rcode = match query.header().opcode() {
        Opcode::Query => Rcode::Refused,
        _ => Rcode::NotImplemented,
    };

    let response = match Message::response_to(&query, rcode, Vec::new()) {
        Ok(response) => response,
        Err(e) => {
            eprintln!("{addr}: {e}");
            return None;
        }
    };

    let mut writer = Writer::new();
    response.write(&mut writer);
    Some(writer.into_bytes())
}

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
}
