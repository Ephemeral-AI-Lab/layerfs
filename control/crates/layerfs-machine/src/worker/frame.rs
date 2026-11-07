//! One frame: a 4-byte big-endian header, then its body.
//!
//! A frame with no byte values is plain: the header is the body's length
//! and the body is JSON. A frame that holds byte values carries them as
//! attachments: the header's top bit ([`ATTACHED`]) is set, its other 31
//! bits are the body's length, and the body is
//! `u32 json length | JSON | u32 count | count x (u64 length | bytes)`, each
//! byte value written `{"attachment": i}` in the JSON, `i` its place among
//! the attachments. Every attachment is named exactly once.
//!
//! Only the request and the frames after it hold byte values: the secret
//! and the worker's answer to it never do, so they are always plain. Both
//! ends of a request speak this build's [`super::PROTOCOL`]: the caller
//! sends nothing to a worker whose `ready` names another (protocol 2 sent
//! bytes as `{"base64": ...}` in a plain frame, and reads an attached
//! header as a length past [`MAX_FRAME`], refused before its body).
use layerfs_bridge::control::{SdkRefusal, SdkValue};
use layerfs_home::json;
use layerfs_home::outcome::refused;
use std::io::{Read, Write};

/// The largest frame body either side accepts, attachments included.
pub const MAX_FRAME: usize = 512 << 20;

/// The largest first frame: the caller's secret, or the worker's answer to
/// it. The worker reads no more than this before the secret matches.
pub const MAX_HELLO: usize = 4 << 10;

/// The header bit of a frame with attachments.
pub const ATTACHED: u32 = 1 << 31;

/// `value` as the bytes of one frame, its byte values as attachments; a
/// frame larger than [`MAX_FRAME`] is refused `TooLarge`, before anything
/// is sent.
pub fn frame_bytes(value: &SdkValue) -> Result<Vec<u8>, SdkRefusal> {
    // the JSON is printed after 8 bytes kept for the header and its length
    let mut text = String::from("\0\0\0\0\0\0\0\0");
    let mut attachments = Vec::new();
    json::write_attached(&mut text, value, &mut attachments);
    let json = text.len() - 8;
    let rest: usize = attachments.iter().map(|bytes| 8 + bytes.len()).sum();
    let length = match attachments.is_empty() {
        true => json,
        false => 4 + json + 4 + rest,
    };
    if length > MAX_FRAME {
        return Err(refused(
            "TooLarge",
            "a frame larger than the worker accepts; nothing was sent",
        ));
    }
    let mut bytes = text.into_bytes();
    if attachments.is_empty() {
        // a plain frame: the length, then the JSON
        bytes.drain(..4);
        bytes[..4].copy_from_slice(&(length as u32).to_be_bytes());
        return Ok(bytes);
    }
    bytes[..4].copy_from_slice(&(length as u32 | ATTACHED).to_be_bytes());
    bytes[4..8].copy_from_slice(&(json as u32).to_be_bytes());
    bytes.reserve_exact(4 + rest);
    bytes.extend_from_slice(&(attachments.len() as u32).to_be_bytes());
    for attachment in attachments {
        bytes.extend_from_slice(&(attachment.len() as u64).to_be_bytes());
        bytes.extend_from_slice(attachment);
    }
    Ok(bytes)
}

/// Writes `value` as one frame.
pub fn write_frame(out: &mut impl Write, value: &SdkValue) -> std::io::Result<()> {
    let bytes = frame_bytes(value).map_err(|refusal| std::io::Error::other(refusal.message))?;
    out.write_all(&bytes)?;
    out.flush()
}

/// Reads one frame of at most [`MAX_FRAME`] bytes; `Ok(None)` at a clean
/// end before any byte.
pub fn read_frame(input: &mut impl Read) -> Result<Option<SdkValue>, SdkRefusal> {
    read_frame_within(input, MAX_FRAME)
}

fn lost(error: std::io::Error) -> SdkRefusal {
    refused("Unavailable", format!("worker connection: {error}"))
}

fn malformed(what: &str) -> SdkRefusal {
    refused("Failed", format!("a worker frame: {what}"))
}

/// Reads one frame whose body is at most `max` bytes. A longer one is
/// refused `TooLarge` from its header alone: none of its body is read. An
/// attached frame's parts are each checked against what is left of its
/// body before they are read.
pub fn read_frame_within(
    input: &mut impl Read,
    max: usize,
) -> Result<Option<SdkValue>, SdkRefusal> {
    let mut header = [0u8; 4];
    match input.read(&mut header[..1]) {
        Ok(0) => return Ok(None),
        Ok(_) => {}
        Err(error) => return Err(lost(error)),
    }
    input.read_exact(&mut header[1..]).map_err(lost)?;
    let header = u32::from_be_bytes(header);
    let length = (header & !ATTACHED) as usize;
    if length > max {
        return Err(refused("TooLarge", "a worker frame is larger than allowed"));
    }
    if header & ATTACHED == 0 {
        return parse(read_bytes(input, length)?).map(Some);
    }
    let mut body = Body {
        input,
        left: length,
    };
    let json = body.u32()? as usize;
    let mut value = parse(body.bytes(json)?)?;
    let count = body.u32()? as usize;
    // each attachment takes at least its 8 length bytes
    if count > body.left / 8 {
        return Err(malformed("more attachments than its length holds"));
    }
    let mut attachments = Vec::with_capacity(count);
    for _ in 0..count {
        let len = usize::try_from(body.u64()?).unwrap_or(usize::MAX);
        attachments.push(Some(body.bytes(len)?));
    }
    if body.left != 0 {
        return Err(malformed("bytes after its last attachment"));
    }
    attach(&mut value, &mut attachments)?;
    if attachments.iter().any(Option::is_some) {
        return Err(malformed("an attachment no value names"));
    }
    Ok(Some(value))
}

/// `length` bytes read into a buffer of that size.
fn read_bytes(input: &mut impl Read, length: usize) -> Result<Vec<u8>, SdkRefusal> {
    let mut bytes = vec![0u8; length];
    input.read_exact(&mut bytes).map_err(lost)?;
    Ok(bytes)
}

fn parse(text: Vec<u8>) -> Result<SdkValue, SdkRefusal> {
    let text = String::from_utf8(text).map_err(|_| malformed("its JSON is not UTF-8"))?;
    json::parse(&text).map_err(|error| malformed(&error.0))
}

/// What is left of an attached frame's body.
struct Body<'a, R> {
    input: &'a mut R,
    left: usize,
}

impl<R: Read> Body<'_, R> {
    /// `len` more bytes of the body; a part longer than what is left is
    /// refused before anything is allocated for it.
    fn bytes(&mut self, len: usize) -> Result<Vec<u8>, SdkRefusal> {
        if len > self.left {
            return Err(malformed("a part longer than its length holds"));
        }
        self.left -= len;
        read_bytes(self.input, len)
    }

    fn u32(&mut self) -> Result<u32, SdkRefusal> {
        let mut four = [0u8; 4];
        four.copy_from_slice(&self.bytes(4)?);
        Ok(u32::from_be_bytes(four))
    }

    fn u64(&mut self) -> Result<u64, SdkRefusal> {
        let mut eight = [0u8; 8];
        eight.copy_from_slice(&self.bytes(8)?);
        Ok(u64::from_be_bytes(eight))
    }
}

/// Puts each attachment where the JSON names it (`{"attachment": i}`),
/// moved, not copied; a place named twice or out of range is refused.
fn attach(value: &mut SdkValue, attachments: &mut [Option<Vec<u8>>]) -> Result<(), SdkRefusal> {
    match value {
        SdkValue::Map(fields) => {
            if let [(name, SdkValue::Int(at))] = fields.as_slice() {
                if name == "attachment" {
                    let slot = usize::try_from(*at)
                        .ok()
                        .and_then(|at| attachments.get_mut(at));
                    let bytes = slot
                        .and_then(Option::take)
                        .ok_or_else(|| malformed("an attachment named twice or not carried"))?;
                    *value = SdkValue::Bytes(bytes);
                    return Ok(());
                }
            }
            for (_, item) in fields {
                attach(item, attachments)?;
            }
            Ok(())
        }
        SdkValue::List(items) => {
            for item in items {
                attach(item, attachments)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}
