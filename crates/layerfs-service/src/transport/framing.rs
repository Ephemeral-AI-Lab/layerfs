use crate::protocol::codec;
use crate::{Result, ServiceError, MAX_WIRE_BYTES};
use std::io::{Read, Write};
use std::net::TcpStream;

pub(crate) fn write_frame<T: serde::Serialize>(stream: &mut TcpStream, value: &T) -> Result<()> {
    let body = codec::encode(value)?;
    stream.write_all(&(body.len() as u32).to_be_bytes())?;
    stream.write_all(&body)?;
    stream.flush()?;
    Ok(())
}

pub(crate) fn read_frame<T: serde::de::DeserializeOwned>(stream: &mut TcpStream) -> Result<T> {
    let mut length = [0; 4];
    stream.read_exact(&mut length)?;
    let length = usize::try_from(u32::from_be_bytes(length))
        .map_err(|_| ServiceError::Wire("request length".into()))?;
    if length == 0 || length > MAX_WIRE_BYTES {
        return Err(ServiceError::Wire("request limit".into()));
    }
    let mut body = vec![0; length];
    stream.read_exact(&mut body)?;
    codec::decode(&body)
}
