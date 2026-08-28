use crate::protocol::request::WireEnvelope;
use crate::protocol::response::WireResponse;
use crate::server::dispatch;
use crate::transport::framing::{read_frame, write_frame};
use crate::{Result, Service};
use std::net::TcpStream;
use std::time::Duration;

pub(crate) fn serve(service: &Service, mut stream: TcpStream) -> Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    stream.set_write_timeout(Some(Duration::from_secs(30)))?;
    let response = match read_frame::<WireEnvelope>(&mut stream)
        .and_then(|envelope| dispatch(service, envelope))
    {
        Ok(response) => response,
        Err(error) => WireResponse::Error(error.to_string()),
    };
    write_frame(&mut stream, &response)
}
