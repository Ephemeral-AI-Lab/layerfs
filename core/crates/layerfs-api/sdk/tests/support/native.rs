use layerfs_bridge::native::{self, Connection};
use std::{
    net::{TcpListener, TcpStream},
    thread,
    time::{Duration, Instant},
};
fn socket(stream: TcpStream) -> TcpStream {
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    stream
}
pub fn pair() -> (Connection, Connection) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    listener.set_nonblocking(true).unwrap();
    let server = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            match listener.accept() {
                Ok((stream, _)) => {
                    return native::accept(
                        socket(stream),
                        &[33; 32],
                        native::public_key(&[1; 32]).unwrap(),
                    )
                    .unwrap()
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline);
                    thread::sleep(Duration::from_millis(1));
                }
                Err(error) => panic!("accept: {error}"),
            }
        }
    });
    let client = native::initiate(
        socket(TcpStream::connect_timeout(&address, Duration::from_secs(2)).unwrap()),
        &[1; 32],
        native::public_key(&[33; 32]).unwrap(),
    )
    .unwrap();
    (client, server.join().unwrap())
}
pub fn poll<T>(mut f: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(value) = f() {
            return value;
        }
        assert!(Instant::now() < deadline, "bounded owner wait");
        thread::sleep(Duration::from_millis(1));
    }
}
