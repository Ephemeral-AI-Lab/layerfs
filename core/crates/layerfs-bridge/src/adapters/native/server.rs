//! Supplied-handler server; connection ownership includes closing work.
use super::{
    connection::{Connection, VerifiedPeer},
    payload::{Input, Output},
    protocol::*,
};
use crate::contract::*;
use std::{
    io::{Read, Write},
    time::{Duration, Instant},
};
pub fn serve(
    mut connection: Connection,
    handler: impl Fn(
        &VerifiedPeer,
        &Request,
        &mut dyn Read,
        &mut dyn Write,
        Instant,
    ) -> Result<Response, Failure>,
) -> Result<(), Failure> {
    let hello = connection.receive.read()?;
    if hello.kind != Kind::Hello || hello.id != 0 || hello.bytes != 1u16.to_be_bytes() {
        return Err(Code::Unsupported.into());
    }
    connection.send.write(&hello)?;
    let mut previous = 0;
    loop {
        let idle = Instant::now() + Duration::from_secs(5);
        connection.receive.deadline(idle);
        connection.send.deadline(idle);
        let begin = connection.receive.read()?;
        if begin.kind != Kind::Begin || begin.id <= previous {
            return Err(Code::InvalidInput.into());
        }
        previous = begin.id;
        let request = match decode_request(begin.id, &begin.bytes) {
            Ok(r) => r,
            Err(e) => {
                let _ = connection.send.write(&Frame {
                    kind: Kind::Failure,
                    id: begin.id,
                    bytes: encode_failure(e.clone()).to_vec(),
                });
                return Err(e);
            }
        };
        let deadline = Instant::now() + Duration::from_millis(request.deadline_ms as u64);
        connection.receive.deadline(deadline);
        connection.send.deadline(deadline);
        let mut input = Input::new(
            &mut connection.receive,
            request.id,
            request.operation.input_length()?,
        );
        let mut output = Output::new(&mut connection.send, request.id, request.response_bytes);
        let result = handler(
            &connection.peer,
            &request,
            &mut input,
            &mut output,
            deadline,
        );
        let result = match result {
            Ok(r) if input.complete() => Ok(r),
            Ok(_) => Err(Code::InvalidInput.into()),
            Err(e) => Err(e),
        };
        match result {
            Ok(response) => connection.send.write(&Frame {
                kind: Kind::Success,
                id: request.id,
                bytes: encode_response(&response)?,
            })?,
            Err(error) => {
                let _ = connection.send.write(&Frame {
                    kind: Kind::Failure,
                    id: request.id,
                    bytes: encode_request_failure(&request, &error)?,
                });
                // Preserve the terminal frame before closing a socket with unread
                // upload bytes. Input enforces the same frame/length/deadline bounds;
                // the client cancels its upload after receiving this failure.
                connection.send.end_upload();
                let _ = std::io::copy(&mut input, &mut std::io::sink());
                connection.receive.close();
                return Err(error);
            }
        }
    }
}
