mod header;
mod hexdump;

use std::io;

use tokio::net::UdpSocket;

use {header::Header, hexdump::Hexdump};

#[tokio::main]
async fn main() -> Result<(), Error> {
    let socket = UdpSocket::bind("127.0.0.1:5353").await?;
    let mut buf = [0u8; 4096];

    loop {
        let (len, addr) = socket.recv_from(&mut buf).await?;
        println!("Received {len} bytes from {addr}");

        println!("Hexdump of the received data:");
        println!("{}", Hexdump::new(&buf[..len]));

        let header = match Header::parse(&buf[..len]) {
            Ok(header) => header,
            Err(e) => {
                eprintln!("Failed to parse header: {e}");
                continue;
            }
        };
        println!("Parsed header: {header:?}");
    }
}

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
}
