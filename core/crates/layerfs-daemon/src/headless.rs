//! Bounded stdin requests and framed stdout; refusal terminates the session.
use layerfs_bridge::{
    adapters::native::{
        client::{Client, Source},
        pipe::Pipe,
        protocol::*,
    },
    contract::*,
};
use std::{
    io::{self, Stdin, Write},
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};
pub fn run(
    client: &mut Client,
    telemetry: &layerfs_telemetry::runtime::Runtime,
) -> Result<(), Failure> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let cancelled = AtomicBool::new(false);
    let mut previous = 0;
    loop {
        let mut pipe = Pipe {
            fd: &stdin,
            deadline: Instant::now() + Duration::from_secs(5),
            cancel: &cancelled,
        };
        let Some(begin) = Frame::read_optional(&mut pipe)? else {
            return Ok(());
        };
        if begin.kind != Kind::Begin || begin.id <= previous {
            return Err(Code::InvalidInput.into());
        }
        previous = begin.id;
        let request = decode_request(begin.id, &begin.bytes)?;
        let deadline = Instant::now() + Duration::from_millis(request.deadline_ms as u64);
        let mut input = Submission {
            stdin: &stdin,
            state: InputState::new(request.id, request.operation.input_length()?),
            buffer: Vec::new(),
            offset: 0,
        };
        let mut output = ResultOutput {
            pipe: Pipe {
                fd: &stdout,
                deadline,
                cancel: &cancelled,
            },
            id: request.id,
        };
        let (result, diagnostic) =
            telemetry
                .recorder()
                .run(request.id, request.operation.label(), |_| {
                    client.call_until(&request, &mut input, &mut output, deadline)
                });
        telemetry.publish(diagnostic);
        match result {
            Ok(response) => Frame {
                kind: Kind::Success,
                id: request.id,
                bytes: encode_response(&response)?,
            }
            .write(&mut output.pipe)?,
            Err(error) => {
                let _ = Frame {
                    kind: Kind::Failure,
                    id: request.id,
                    bytes: encode_failure(error).to_vec(),
                }
                .write(&mut output.pipe);
                return Err(error);
            }
        }
    }
}
struct Submission<'a> {
    stdin: &'a Stdin,
    state: InputState,
    buffer: Vec<u8>,
    offset: usize,
}
impl Source for Submission<'_> {
    fn read(
        &mut self,
        out: &mut [u8],
        deadline: Instant,
        cancel: &AtomicBool,
    ) -> io::Result<usize> {
        if out.is_empty() || self.state.ended() {
            return Ok(0);
        }
        if self.offset == self.buffer.len() {
            let mut pipe = Pipe {
                fd: self.stdin,
                deadline,
                cancel,
            };
            let frame = Frame::read(&mut pipe).map_err(io::Error::other)?;
            self.state.accept(&frame).map_err(io::Error::other)?;
            if self.state.ended() {
                return Ok(0);
            }
            self.buffer = frame.bytes;
            self.offset = 0;
        }
        let n = out.len().min(self.buffer.len() - self.offset);
        out[..n].copy_from_slice(&self.buffer[self.offset..self.offset + n]);
        self.offset += n;
        Ok(n)
    }
}
struct ResultOutput<'a> {
    pipe: Pipe<'a, io::Stdout>,
    id: u64,
}
impl Write for ResultOutput<'_> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        for chunk in b.chunks(FRAME_BYTES) {
            Frame {
                kind: Kind::ResultData,
                id: self.id,
                bytes: chunk.to_vec(),
            }
            .write(&mut self.pipe)
            .map_err(io::Error::other)?;
        }
        Ok(b.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
