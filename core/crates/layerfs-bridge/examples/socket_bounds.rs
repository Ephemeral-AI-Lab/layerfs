//! External native socket-buffer diagnostic, without a service or protocol peer.
use nix::sys::socket::{getsockopt, setsockopt, sockopt};
use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    time::Duration,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let server = std::thread::spawn(move || {
        for _ in 0..64 {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(1)))
                .unwrap();
            let _ = stream.read(&mut [0; 1]);
        }
    });
    let mut oversized = 0;
    for index in 0..64 {
        let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(1))?;
        let before = (
            getsockopt(&stream, sockopt::SndBuf)?,
            getsockopt(&stream, sockopt::RcvBuf)?,
        );
        setsockopt(&stream, sockopt::SndBuf, &(128 * 1024))?;
        setsockopt(&stream, sockopt::RcvBuf, &(128 * 1024))?;
        let after = (
            getsockopt(&stream, sockopt::SndBuf)?,
            getsockopt(&stream, sockopt::RcvBuf)?,
        );
        if after.0 > 256 * 1024 || after.1 > 256 * 1024 {
            oversized += 1;
            println!("socket {index}: before={before:?} after={after:?}");
        }
        stream.write_all(&[0])?;
    }
    server.join().unwrap();
    println!("64 sockets; over-bound after successful setsockopt: {oversized}");
    Ok(())
}
