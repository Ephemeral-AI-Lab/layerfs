//! A netlink socket and its message codec, in safe Rust over `nix`
//! sockets: no netlink crate exists in the lockfile, and the routing and
//! nftables messages LayerFS sends are few and fixed. A socket belongs to
//! the network namespace it was opened in for its whole life, which the
//! init stage relies on to configure the machine's side after it entered
//! the Sandbox's namespace. Every request is sent once and its answer read
//! once: nothing is resent.
use nix::sys::socket::{
    bind, recv, send, socket, AddressFamily, MsgFlags, NetlinkAddr, SockFlag, SockProtocol,
    SockType,
};
use std::io;
use std::os::fd::{AsRawFd, OwnedFd};

pub(crate) const NLM_F_REQUEST: u16 = 0x1;
pub(crate) const NLM_F_ACK: u16 = 0x4;
pub(crate) const NLM_F_DUMP: u16 = 0x300;
pub(crate) const NLM_F_EXCL: u16 = 0x200;
pub(crate) const NLM_F_CREATE: u16 = 0x400;
pub(crate) const NLM_F_APPEND: u16 = 0x800;
const NLMSG_ERROR: u16 = 2;
const NLMSG_DONE: u16 = 3;
/// An attribute that holds attributes.
const NLA_F_NESTED: u16 = 1 << 15;
/// The bits of an attribute's type that name it.
const NLA_TYPE_MASK: u16 = !(NLA_F_NESTED | (1 << 14));
const HEADER: usize = 16;

fn align(length: usize) -> usize {
    (length + 3) & !3
}

/// One request message being built: its header, a fixed family header and
/// attributes, nested ones included.
pub(crate) struct Message {
    buffer: Vec<u8>,
    open: Vec<usize>,
}

impl Message {
    /// A message of type `kind` with `flags` (`NLM_F_REQUEST` is added).
    pub(crate) fn new(kind: u16, flags: u16) -> Self {
        let mut buffer = vec![0u8; HEADER];
        buffer[4..6].copy_from_slice(&kind.to_ne_bytes());
        buffer[6..8].copy_from_slice(&(flags | NLM_F_REQUEST).to_ne_bytes());
        Self {
            buffer,
            open: Vec::new(),
        }
    }

    /// Fixed bytes (a family header such as `ifinfomsg`), padded.
    pub(crate) fn raw(mut self, bytes: &[u8]) -> Self {
        self.buffer.extend_from_slice(bytes);
        self.pad();
        self
    }

    fn pad(&mut self) {
        let length = align(self.buffer.len());
        self.buffer.resize(length, 0);
    }

    /// One attribute of type `kind` holding `data`.
    pub(crate) fn attr(mut self, kind: u16, data: &[u8]) -> Self {
        let length = (4 + data.len()) as u16;
        self.buffer.extend_from_slice(&length.to_ne_bytes());
        self.buffer.extend_from_slice(&kind.to_ne_bytes());
        self.buffer.extend_from_slice(data);
        self.pad();
        self
    }

    /// A NUL-terminated string attribute.
    pub(crate) fn text(self, kind: u16, text: &str) -> Self {
        let mut data = text.as_bytes().to_vec();
        data.push(0);
        self.attr(kind, &data)
    }

    /// A process-endian `u32` attribute (routing messages).
    pub(crate) fn u32(self, kind: u16, value: u32) -> Self {
        self.attr(kind, &value.to_ne_bytes())
    }

    /// A big-endian `u32` attribute (nftables messages).
    pub(crate) fn be32(self, kind: u16, value: u32) -> Self {
        self.attr(kind, &value.to_be_bytes())
    }

    /// Opens a nested attribute of type `kind`; [`Self::end`] closes it.
    pub(crate) fn begin(mut self, kind: u16) -> Self {
        self.open.push(self.buffer.len());
        self.buffer.extend_from_slice(&[0, 0]);
        self.buffer
            .extend_from_slice(&(kind | NLA_F_NESTED).to_ne_bytes());
        self
    }

    /// Closes the innermost open nested attribute.
    pub(crate) fn end(mut self) -> Self {
        if let Some(start) = self.open.pop() {
            let length = (self.buffer.len() - start) as u16;
            self.buffer[start..start + 2].copy_from_slice(&length.to_ne_bytes());
        }
        self
    }

    /// The finished bytes with sequence number `sequence`.
    pub(crate) fn bytes(mut self, sequence: u32) -> Vec<u8> {
        let length = self.buffer.len() as u32;
        self.buffer[0..4].copy_from_slice(&length.to_ne_bytes());
        self.buffer[8..12].copy_from_slice(&sequence.to_ne_bytes());
        self.buffer
    }

    fn acked(&self) -> bool {
        u16::from_ne_bytes([self.buffer[6], self.buffer[7]]) & NLM_F_ACK != 0
    }
}

/// The attributes in `bytes`, each as its type (flags masked) and data.
pub(crate) fn attributes(mut bytes: &[u8]) -> Vec<(u16, &[u8])> {
    let mut found = Vec::new();
    while bytes.len() >= 4 {
        let length = usize::from(u16::from_ne_bytes([bytes[0], bytes[1]]));
        let kind = u16::from_ne_bytes([bytes[2], bytes[3]]) & NLA_TYPE_MASK;
        if length < 4 || length > bytes.len() {
            break;
        }
        found.push((kind, &bytes[4..length]));
        bytes = &bytes[align(length).min(bytes.len())..];
    }
    found
}

/// The attribute of type `kind` in `attributes`, if any.
pub(crate) fn find<'a>(attributes: &[(u16, &'a [u8])], kind: u16) -> Option<&'a [u8]> {
    attributes
        .iter()
        .find(|(found, _)| *found == kind)
        .map(|(_, data)| *data)
}

/// A string attribute's text, without its NUL.
pub(crate) fn text(data: &[u8]) -> String {
    let end = data.iter().position(|b| *b == 0).unwrap_or(data.len());
    String::from_utf8_lossy(&data[..end]).into_owned()
}

/// A process-endian `u32` attribute's value.
pub(crate) fn u32_of(data: &[u8]) -> Option<u32> {
    Some(u32::from_ne_bytes(data.get(..4)?.try_into().ok()?))
}

/// One message the kernel answered: its type and payload.
pub(crate) struct Reply {
    pub(crate) kind: u16,
    pub(crate) payload: Vec<u8>,
}

/// One netlink socket of `protocol`, in the network namespace of the thread
/// that opened it.
pub(crate) struct Socket {
    fd: OwnedFd,
    sequence: u32,
}

impl Socket {
    pub(crate) fn open(protocol: SockProtocol) -> io::Result<Self> {
        let fd = socket(
            AddressFamily::Netlink,
            SockType::Raw,
            SockFlag::SOCK_CLOEXEC,
            protocol,
        )?;
        bind(fd.as_raw_fd(), &NetlinkAddr::new(0, 0))?;
        Ok(Self { fd, sequence: 1 })
    }

    fn next(&mut self) -> u32 {
        self.sequence = self.sequence.wrapping_add(1);
        self.sequence
    }

    /// Sends `message` (with `NLM_F_ACK`) once and waits for its answer.
    pub(crate) fn ack(&mut self, message: Message) -> io::Result<()> {
        self.batch(vec![message])
    }

    /// Sends `messages` in one write and waits until each one that asked
    /// for an acknowledgement has one; the first error ends the wait with
    /// that error (nftables then applied none of the batch).
    pub(crate) fn batch(&mut self, messages: Vec<Message>) -> io::Result<()> {
        let mut bytes = Vec::new();
        let mut waiting = Vec::new();
        for message in messages {
            let sequence = self.next();
            if message.acked() {
                waiting.push(sequence);
            }
            bytes.extend(message.bytes(sequence));
        }
        self.send(&bytes)?;
        while !waiting.is_empty() {
            for (kind, sequence, payload) in self.receive()? {
                if kind != NLMSG_ERROR {
                    continue;
                }
                let code = payload
                    .get(..4)
                    .map(|b| i32::from_ne_bytes([b[0], b[1], b[2], b[3]]))
                    .unwrap_or(-libc_eio());
                if code != 0 {
                    return Err(io::Error::from_raw_os_error(-code));
                }
                waiting.retain(|seen| *seen != sequence);
            }
        }
        Ok(())
    }

    /// Sends the dump request `message` once and returns every message the
    /// kernel answered until its end.
    pub(crate) fn dump(&mut self, message: Message) -> io::Result<Vec<Reply>> {
        let sequence = self.next();
        self.send(&message.bytes(sequence))?;
        let mut replies = Vec::new();
        loop {
            for (kind, seen, payload) in self.receive()? {
                if seen != sequence {
                    continue;
                }
                match kind {
                    NLMSG_DONE => return Ok(replies),
                    NLMSG_ERROR => {
                        let code = payload
                            .get(..4)
                            .map(|b| i32::from_ne_bytes([b[0], b[1], b[2], b[3]]))
                            .unwrap_or(-libc_eio());
                        return Err(io::Error::from_raw_os_error(-code));
                    }
                    _ => replies.push(Reply { kind, payload }),
                }
            }
        }
    }

    fn send(&self, bytes: &[u8]) -> io::Result<()> {
        let sent = send(self.fd.as_raw_fd(), bytes, MsgFlags::empty())?;
        if sent != bytes.len() {
            return Err(io::Error::new(
                io::ErrorKind::WriteZero,
                "a netlink request was sent in part",
            ));
        }
        Ok(())
    }

    /// One read of the socket, split into its messages.
    fn receive(&self) -> io::Result<Vec<(u16, u32, Vec<u8>)>> {
        let mut buffer = vec![0u8; 1 << 16];
        let length = recv(self.fd.as_raw_fd(), &mut buffer, MsgFlags::empty())?;
        let mut rest = &buffer[..length];
        let mut messages = Vec::new();
        while rest.len() >= HEADER {
            let size = u32::from_ne_bytes([rest[0], rest[1], rest[2], rest[3]]) as usize;
            if size < HEADER || size > rest.len() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "a malformed netlink answer",
                ));
            }
            let kind = u16::from_ne_bytes([rest[4], rest[5]]);
            let sequence = u32::from_ne_bytes([rest[8], rest[9], rest[10], rest[11]]);
            messages.push((kind, sequence, rest[HEADER..size].to_vec()));
            rest = &rest[align(size).min(rest.len())..];
        }
        Ok(messages)
    }
}

fn libc_eio() -> i32 {
    nix::libc::EIO
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_encode_headers_padding_and_nesting() {
        let bytes = Message::new(16, NLM_F_ACK)
            .raw(&[1, 2, 3])
            .begin(18)
            .text(1, "veth")
            .end()
            .bytes(7);
        // header: length, type 16, flags REQUEST|ACK, sequence 7, pid 0
        assert_eq!(
            u32::from_ne_bytes(bytes[0..4].try_into().unwrap()) as usize,
            bytes.len()
        );
        assert_eq!(u16::from_ne_bytes([bytes[4], bytes[5]]), 16);
        assert_eq!(
            u16::from_ne_bytes([bytes[6], bytes[7]]),
            NLM_F_REQUEST | NLM_F_ACK
        );
        assert_eq!(u32::from_ne_bytes(bytes[8..12].try_into().unwrap()), 7);
        // the fixed header is padded to 4 bytes
        assert_eq!(&bytes[16..20], &[1, 2, 3, 0]);
        // nested: length 4 + (4 + "veth\0", padded to 12) = 16, type with NESTED
        assert_eq!(u16::from_ne_bytes([bytes[20], bytes[21]]), 16);
        assert_eq!(
            u16::from_ne_bytes([bytes[22], bytes[23]]),
            18 | NLA_F_NESTED
        );
        let inner = attributes(&bytes[24..]);
        assert_eq!(inner.len(), 1);
        assert_eq!(text(inner[0].1), "veth");
        let outer = attributes(&bytes[20..]);
        assert_eq!(outer[0].0, 18);
    }
}
