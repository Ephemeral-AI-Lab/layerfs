//! Exact HTTP framing; chunk sizes are counters, never allocations.
use super::headers::field;
use std::io::{self, Read};
pub(super) enum Framing {
    Fixed(u64),
    Chunked,
    Close,
    Hijacked,
}
pub(super) struct Body<R> {
    inner: R,
    mode: Framing,
    remaining: u64,
    post_chunk: bool,
    done: bool,
}
impl<R: Read> Body<R> {
    pub fn new(inner: R, mode: Framing) -> Self {
        Self {
            inner,
            mode,
            remaining: 0,
            post_chunk: false,
            done: false,
        }
    }
    fn next_chunk(&mut self) -> io::Result<()> {
        if self.post_chunk {
            let mut crlf = [0; 2];
            exact(&mut self.inner, &mut crlf)?;
            if crlf != *b"\r\n" {
                return Err(invalid("chunk payload CRLF"));
            }
            self.post_chunk = false;
        }
        let size_line = line(&mut self.inner, 8192)?;
        // The negotiated Engine profile uses plain chunk sizes. Extensions
        // are explicitly unsupported, never loosely parsed/ignored.
        if size_line.contains(&b';') {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Engine chunk extensions",
            ));
        }
        let size = size_line.as_slice();
        if size.is_empty() {
            return Err(invalid("empty chunk size"));
        }
        let mut value = 0u64;
        for &b in size {
            let n = match b {
                b'0'..=b'9' => b - b'0',
                b'a'..=b'f' => b - b'a' + 10,
                b'A'..=b'F' => b - b'A' + 10,
                _ => return Err(invalid("chunk size digit")),
            };
            value = value
                .checked_mul(16)
                .and_then(|v| v.checked_add(u64::from(n)))
                .ok_or_else(|| invalid("chunk size width"))?;
        }
        if size_line.iter().any(|b| *b < 32 || *b == 127) {
            return Err(invalid("chunk extension control"));
        }
        if value == 0 {
            let mut total = 0usize;
            loop {
                let trailer = line(&mut self.inner, 8192)?;
                total = total
                    .checked_add(trailer.len() + 2)
                    .ok_or_else(|| invalid("trailer width"))?;
                if total > 32768 {
                    return Err(invalid("trailer metadata window"));
                }
                if trailer.is_empty() {
                    break;
                }
                let (name, _) = field(&trailer)?;
                if [
                    b"content-length".as_slice(),
                    b"transfer-encoding",
                    b"connection",
                    b"upgrade",
                    b"content-type",
                ]
                .iter()
                .any(|value| name.eq_ignore_ascii_case(value))
                {
                    return Err(invalid("framing trailer"));
                }
            }
            self.done = true;
        } else {
            self.remaining = value;
            self.post_chunk = true;
        }
        Ok(())
    }
}
impl<R: Read> Read for Body<R> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() || self.done {
            return Ok(0);
        }
        match &mut self.mode {
            Framing::Fixed(left) => {
                if *left == 0 {
                    self.done = true;
                    return Ok(0);
                }
                let n = out.len().min(usize::try_from(*left).unwrap_or(usize::MAX));
                let read = self.inner.read(&mut out[..n])?;
                if read == 0 {
                    return Err(io::ErrorKind::UnexpectedEof.into());
                }
                *left -= read as u64;
                Ok(read)
            }
            Framing::Close | Framing::Hijacked => self.inner.read(out),
            Framing::Chunked => {
                if self.remaining == 0 {
                    self.next_chunk()?
                }
                if self.done {
                    return Ok(0);
                }
                let n = out
                    .len()
                    .min(usize::try_from(self.remaining).unwrap_or(usize::MAX));
                let read = self.inner.read(&mut out[..n])?;
                if read == 0 {
                    return Err(io::ErrorKind::UnexpectedEof.into());
                }
                self.remaining -= read as u64;
                Ok(read)
            }
        }
    }
}
pub(super) fn exact(reader: &mut impl Read, out: &mut [u8]) -> io::Result<()> {
    let mut done = 0;
    while done < out.len() {
        let n = reader.read(&mut out[done..])?;
        if n == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        done += n;
    }
    Ok(())
}
pub(super) fn line(reader: &mut impl Read, limit: usize) -> io::Result<Vec<u8>> {
    let mut out = Vec::with_capacity(limit.min(256));
    loop {
        let mut one = [0];
        exact(reader, &mut one)?;
        if one[0] == b'\r' {
            exact(reader, &mut one)?;
            if one[0] != b'\n' {
                return Err(invalid("line CRLF"));
            }
            return Ok(out);
        }
        if one[0] == b'\n' || out.len() == limit {
            return Err(invalid("header line window/CRLF"));
        }
        out.push(one[0]);
    }
}
fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
