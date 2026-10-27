//! A minimal NFSv3/MOUNT client over the export's public XDR and record
//! framing: what a kernel client sends, one call at a time.
use layerfs_nfs::{
    rpc::{read_record, write_record},
    xdr::{Reader, Writer},
};
use std::{
    net::TcpStream,
    sync::{Arc, Condvar, Mutex},
    time::Duration,
};

/// Calls sent and replies fully received by clients sharing it, so a test's
/// port wrapper can tell whether a reply was already on the wire.
#[derive(Default)]
pub struct Wire {
    pub counts: Mutex<(u64, u64)>,
    pub changed: Condvar,
}

pub const NFS: u32 = 100_003;
pub const MOUNT: u32 = 100_005;
pub const OK: u32 = 0;
pub const NOENT: u32 = 2;
pub const ROFS: u32 = 30;
pub const STALE: u32 = 70;
pub const BADHANDLE: u32 = 10001;
pub const BAD_COOKIE: u32 = 10003;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Attr {
    pub kind: u32,
    pub mode: u32,
    pub nlink: u32,
    pub uid: u32,
    pub gid: u32,
    pub size: u64,
    pub fileid: u64,
    pub mtime: (u32, u32),
}
fn attr(reader: &mut Reader<'_>) -> Attr {
    let kind = reader.u32().unwrap();
    let mode = reader.u32().unwrap();
    let nlink = reader.u32().unwrap();
    let uid = reader.u32().unwrap();
    let gid = reader.u32().unwrap();
    let size = reader.u64().unwrap();
    let _used = reader.u64().unwrap();
    let _rdev = (reader.u32().unwrap(), reader.u32().unwrap());
    let _fsid = reader.u64().unwrap();
    let fileid = reader.u64().unwrap();
    let _atime = (reader.u32().unwrap(), reader.u32().unwrap());
    let mtime = (reader.u32().unwrap(), reader.u32().unwrap());
    let _ctime = (reader.u32().unwrap(), reader.u32().unwrap());
    Attr {
        kind,
        mode,
        nlink,
        uid,
        gid,
        size,
        fileid,
        mtime,
    }
}
fn post_op(reader: &mut Reader<'_>) -> Option<Attr> {
    reader.bool().unwrap().then(|| attr(reader))
}
/// A LOOKUP answer: its status, the object's handle and attributes when
/// found, and the directory's attributes.
pub type Lookup = (u32, Option<(Vec<u8>, Attr)>, Option<Attr>);
/// One READDIR/READDIRPLUS entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Entry {
    pub name: Vec<u8>,
    pub fileid: u64,
    pub cookie: u64,
}

pub struct Client {
    stream: TcpStream,
    xid: u32,
    uid: u32,
    gid: u32,
    pub calls: u64,
    pub wire: Option<Arc<Wire>>,
}
impl Client {
    pub fn connect(address: std::net::SocketAddr, uid: u32, gid: u32) -> Self {
        let stream = TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(30)))
            .unwrap();
        Self {
            stream,
            xid: 1,
            uid,
            gid,
            calls: 0,
            wire: None,
        }
    }
    /// One call; the accept status and the results after it.
    pub fn call(&mut self, program: u32, procedure: u32, arguments: &[u8]) -> (u32, Vec<u8>) {
        self.xid += 1;
        self.calls += 1;
        let mut credential = Writer::new();
        credential
            .u32(0)
            .opaque(b"test")
            .u32(self.uid)
            .u32(self.gid)
            .u32(0);
        let mut call = Writer::new();
        call.u32(self.xid)
            .u32(0)
            .u32(2)
            .u32(program)
            .u32(3)
            .u32(procedure);
        call.u32(1).opaque(credential.as_bytes());
        call.u32(0).u32(0);
        call.fixed(arguments);
        if let Some(wire) = &self.wire {
            wire.counts.lock().unwrap().0 += 1;
        }
        write_record(&mut self.stream, call.as_bytes()).unwrap();
        let record = read_record(&mut self.stream).unwrap().unwrap();
        if let Some(wire) = &self.wire {
            wire.counts.lock().unwrap().1 += 1;
            wire.changed.notify_all();
        }
        let mut reader = Reader::new(&record);
        assert_eq!(reader.u32().unwrap(), self.xid);
        assert_eq!(reader.u32().unwrap(), 1, "a reply");
        assert_eq!(reader.u32().unwrap(), 0, "accepted");
        let _flavor = reader.u32().unwrap();
        let _verifier = reader.opaque(400).unwrap();
        let status = reader.u32().unwrap();
        let rest = record[record.len() - reader.remaining()..].to_vec();
        (status, rest)
    }
    /// One NFS procedure that the RPC layer accepted: its nfsstat3 and results.
    pub fn nfs(&mut self, procedure: u32, arguments: &Writer) -> (u32, Vec<u8>) {
        let (accept, rest) = self.call(NFS, procedure, arguments.as_bytes());
        assert_eq!(accept, 0, "procedure {procedure} accepted");
        let mut reader = Reader::new(&rest);
        let status = reader.u32().unwrap();
        (status, rest[4..].to_vec())
    }
    pub fn mnt(&mut self, path: &str) -> Vec<u8> {
        let mut arguments = Writer::new();
        arguments.opaque(path.as_bytes());
        let (accept, rest) = self.call(MOUNT, 1, arguments.as_bytes());
        assert_eq!(accept, 0);
        let mut reader = Reader::new(&rest);
        assert_eq!(reader.u32().unwrap(), 0, "MNT3_OK");
        reader.opaque(64).unwrap().to_vec()
    }
    pub fn getattr(&mut self, handle: &[u8]) -> Result<Attr, u32> {
        let mut arguments = Writer::new();
        arguments.opaque(handle);
        let (status, rest) = self.nfs(1, &arguments);
        if status != OK {
            return Err(status);
        }
        Ok(attr(&mut Reader::new(&rest)))
    }
    pub fn lookup(&mut self, directory: &[u8], name: &[u8]) -> Result<(Vec<u8>, Attr), u32> {
        let mut arguments = Writer::new();
        arguments.opaque(directory).opaque(name);
        let (status, rest) = self.nfs(3, &arguments);
        if status != OK {
            return Err(status);
        }
        let mut reader = Reader::new(&rest);
        let handle = reader.opaque(64).unwrap().to_vec();
        let attributes = post_op(&mut reader).expect("LOOKUP returns object attributes");
        Ok((handle, attributes))
    }
    /// LOOKUP with both halves of its answer: the status, the object's
    /// handle and attributes when found, and the directory's attributes.
    pub fn lookup_with_directory(&mut self, directory: &[u8], name: &[u8]) -> Lookup {
        let mut arguments = Writer::new();
        arguments.opaque(directory).opaque(name);
        let (status, rest) = self.nfs(3, &arguments);
        let mut reader = Reader::new(&rest);
        let object = (status == OK).then(|| {
            let handle = reader.opaque(64).unwrap().to_vec();
            (
                handle,
                post_op(&mut reader).expect("LOOKUP returns object attributes"),
            )
        });
        let parent = post_op(&mut reader);
        (status, object, parent)
    }
    pub fn access(&mut self, handle: &[u8], requested: u32) -> Result<u32, u32> {
        let mut arguments = Writer::new();
        arguments.opaque(handle).u32(requested);
        let (status, rest) = self.nfs(4, &arguments);
        if status != OK {
            return Err(status);
        }
        let mut reader = Reader::new(&rest);
        post_op(&mut reader);
        Ok(reader.u32().unwrap())
    }
    pub fn readlink(&mut self, handle: &[u8]) -> Result<Vec<u8>, u32> {
        let mut arguments = Writer::new();
        arguments.opaque(handle);
        let (status, rest) = self.nfs(5, &arguments);
        if status != OK {
            return Err(status);
        }
        let mut reader = Reader::new(&rest);
        post_op(&mut reader);
        Ok(reader.opaque(4096).unwrap().to_vec())
    }
    /// One READ: data and EOF.
    pub fn read(&mut self, handle: &[u8], offset: u64, count: u32) -> Result<(Vec<u8>, bool), u32> {
        let mut arguments = Writer::new();
        arguments.opaque(handle).u64(offset).u32(count);
        let (status, rest) = self.nfs(6, &arguments);
        if status != OK {
            return Err(status);
        }
        let mut reader = Reader::new(&rest);
        post_op(&mut reader);
        let count = reader.u32().unwrap();
        let eof = reader.bool().unwrap();
        let data = reader.opaque(1 << 20).unwrap().to_vec();
        assert_eq!(count as usize, data.len());
        Ok((data, eof))
    }
    /// READs until EOF in windows of `count` bytes.
    pub fn read_all(&mut self, handle: &[u8], count: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        loop {
            let (data, eof) = self.read(handle, bytes.len() as u64, count).unwrap();
            bytes.extend_from_slice(&data);
            if eof {
                return bytes;
            }
            assert!(!data.is_empty(), "a short non-EOF read makes progress");
        }
    }
    /// One READDIR (or READDIRPLUS): entries, verifier, EOF.
    pub fn readdir(
        &mut self,
        handle: &[u8],
        cookie: u64,
        verifier: u64,
        budget: u32,
        plus: bool,
    ) -> Result<(Vec<Entry>, u64, bool), u32> {
        let mut arguments = Writer::new();
        arguments
            .opaque(handle)
            .u64(cookie)
            .u64(verifier)
            .u32(budget);
        if plus {
            arguments.u32(budget);
        }
        let (status, rest) = self.nfs(if plus { 17 } else { 16 }, &arguments);
        if status != OK {
            return Err(status);
        }
        let mut reader = Reader::new(&rest);
        post_op(&mut reader);
        let verifier = reader.u64().unwrap();
        let mut entries = Vec::new();
        while reader.bool().unwrap() {
            let fileid = reader.u64().unwrap();
            let name = reader.opaque(4096).unwrap().to_vec();
            let cookie = reader.u64().unwrap();
            if plus {
                assert!(post_op(&mut reader).is_none());
                assert!(!reader.bool().unwrap(), "no handle in READDIRPLUS");
            }
            entries.push(Entry {
                name,
                fileid,
                cookie,
            });
        }
        let eof = reader.bool().unwrap();
        Ok((entries, verifier, eof))
    }
    /// A whole enumeration as a kernel client runs it, and the calls it took.
    pub fn list(&mut self, handle: &[u8], budget: u32, plus: bool) -> (Vec<Entry>, u64) {
        let (mut cookie, mut verifier, mut calls) = (0, 0, 0);
        let mut all = Vec::new();
        loop {
            let (entries, next, eof) = self
                .readdir(handle, cookie, verifier, budget, plus)
                .unwrap();
            calls += 1;
            verifier = next;
            if let Some(last) = entries.last() {
                cookie = last.cookie;
            }
            all.extend(entries);
            if eof {
                return (all, calls);
            }
        }
    }
}

pub const PERM: u32 = 1;
pub const EXIST: u32 = 17;
pub const NOTDIR: u32 = 20;
pub const ISDIR: u32 = 21;
pub const INVAL: u32 = 22;
pub const NAMETOOLONG: u32 = 63;
pub const NOTEMPTY: u32 = 66;
pub const ACCES: u32 = 13;
pub const NOT_SYNC: u32 = 10002;
pub const NOTSUPP: u32 = 10004;
pub const JUKEBOX: u32 = 10008;

/// `wcc_attr`: size, mtime and ctime before a change.
pub type Before = (u64, (u32, u32), (u32, u32));
/// `wcc_data`: the pre-op (size, mtime, ctime) and post-op attributes.
#[derive(Clone, Debug, Default)]
pub struct Wcc {
    pub before: Option<Before>,
    pub after: Option<Attr>,
}
pub fn wcc(reader: &mut Reader<'_>) -> Wcc {
    let before = reader.bool().unwrap().then(|| {
        let size = reader.u64().unwrap();
        let mtime = (reader.u32().unwrap(), reader.u32().unwrap());
        let ctime = (reader.u32().unwrap(), reader.u32().unwrap());
        (size, mtime, ctime)
    });
    Wcc {
        before,
        after: post_op(reader),
    }
}
/// A `sattr3` to send.
#[derive(Clone, Copy, Debug, Default)]
pub struct Set {
    pub mode: Option<u32>,
    pub uid: Option<u32>,
    pub gid: Option<u32>,
    pub size: Option<u64>,
    /// None: don't change; Some(None): server time; Some(Some(t)): client time.
    pub mtime: Option<Option<(u32, u32)>>,
}
pub fn sattr(out: &mut Writer, set: Set) {
    let field = |out: &mut Writer, value: Option<u32>| match value {
        Some(value) => {
            out.bool(true).u32(value);
        }
        None => {
            out.bool(false);
        }
    };
    field(out, set.mode);
    field(out, set.uid);
    field(out, set.gid);
    match set.size {
        Some(size) => {
            out.bool(true).u64(size);
        }
        None => {
            out.bool(false);
        }
    }
    out.u32(0); // atime: don't change
    match set.mtime {
        None => {
            out.u32(0);
        }
        Some(None) => {
            out.u32(1);
        }
        Some(Some((seconds, nanoseconds))) => {
            out.u32(2).u32(seconds).u32(nanoseconds);
        }
    }
}
/// The results of CREATE, MKDIR and SYMLINK.
#[derive(Clone, Debug)]
pub struct Made {
    pub handle: Vec<u8>,
    pub attr: Attr,
    pub directory: Wcc,
}
/// `createhow3`.
#[derive(Clone, Copy, Debug)]
pub enum How {
    Unchecked(Set),
    Guarded(Set),
    Exclusive([u8; 8]),
}
impl Client {
    fn made(&mut self, procedure: u32, arguments: &Writer) -> Result<Made, (u32, Wcc)> {
        let (status, rest) = self.nfs(procedure, arguments);
        let mut reader = Reader::new(&rest);
        if status != OK {
            return Err((status, wcc(&mut reader)));
        }
        assert!(reader.bool().unwrap(), "a created handle");
        let handle = reader.opaque(64).unwrap().to_vec();
        let attr = post_op(&mut reader).expect("created attributes");
        let directory = wcc(&mut reader);
        Ok(Made {
            handle,
            attr,
            directory,
        })
    }
    pub fn create(&mut self, directory: &[u8], name: &[u8], how: How) -> Result<Made, u32> {
        let mut arguments = Writer::new();
        arguments.opaque(directory).opaque(name);
        match how {
            How::Unchecked(set) => {
                arguments.u32(0);
                sattr(&mut arguments, set);
            }
            How::Guarded(set) => {
                arguments.u32(1);
                sattr(&mut arguments, set);
            }
            How::Exclusive(verifier) => {
                arguments.u32(2).fixed(&verifier);
            }
        }
        self.made(8, &arguments).map_err(|(status, _)| status)
    }
    /// CREATE UNCHECKED with a mode, the common case.
    pub fn touch(&mut self, directory: &[u8], name: &[u8], mode: u32) -> Result<Made, u32> {
        let set = Set {
            mode: Some(mode),
            ..Set::default()
        };
        self.create(directory, name, How::Unchecked(set))
    }
    pub fn mkdir(&mut self, directory: &[u8], name: &[u8], mode: u32) -> Result<Made, u32> {
        let mut arguments = Writer::new();
        arguments.opaque(directory).opaque(name);
        let set = Set {
            mode: Some(mode),
            ..Set::default()
        };
        sattr(&mut arguments, set);
        self.made(9, &arguments).map_err(|(status, _)| status)
    }
    pub fn symlink(&mut self, directory: &[u8], name: &[u8], target: &[u8]) -> Result<Made, u32> {
        let mut arguments = Writer::new();
        arguments.opaque(directory).opaque(name);
        sattr(&mut arguments, Set::default());
        arguments.opaque(target);
        self.made(10, &arguments).map_err(|(status, _)| status)
    }
    pub fn mknod_fifo(&mut self, directory: &[u8], name: &[u8]) -> u32 {
        let mut arguments = Writer::new();
        arguments.opaque(directory).opaque(name).u32(7);
        sattr(&mut arguments, Set::default());
        self.nfs(11, &arguments).0
    }
    /// SETATTR, optionally guarded by a ctime.
    pub fn setattr(
        &mut self,
        handle: &[u8],
        set: Set,
        guard: Option<(u32, u32)>,
    ) -> Result<Wcc, (u32, Wcc)> {
        let mut arguments = Writer::new();
        arguments.opaque(handle);
        sattr(&mut arguments, set);
        match guard {
            Some((seconds, nanoseconds)) => {
                arguments.bool(true).u32(seconds).u32(nanoseconds);
            }
            None => {
                arguments.bool(false);
            }
        }
        let (status, rest) = self.nfs(2, &arguments);
        let found = wcc(&mut Reader::new(&rest));
        if status == OK {
            Ok(found)
        } else {
            Err((status, found))
        }
    }
    /// WRITE: (count, committed, verifier, file wcc).
    pub fn write(
        &mut self,
        handle: &[u8],
        offset: u64,
        data: &[u8],
    ) -> Result<(u32, u32, [u8; 8], Wcc), u32> {
        let mut arguments = Writer::new();
        arguments
            .opaque(handle)
            .u64(offset)
            .u32(data.len() as u32)
            .u32(0)
            .opaque(data);
        let (status, rest) = self.nfs(7, &arguments);
        if status != OK {
            return Err(status);
        }
        let mut reader = Reader::new(&rest);
        let file = wcc(&mut reader);
        let count = reader.u32().unwrap();
        let committed = reader.u32().unwrap();
        let mut verifier = [0; 8];
        verifier.copy_from_slice(reader.fixed(8).unwrap());
        Ok((count, committed, verifier, file))
    }
    /// WRITE of everything, in pieces of `piece` bytes.
    pub fn write_all(&mut self, handle: &[u8], offset: u64, data: &[u8], piece: usize) {
        let mut at = 0;
        while at < data.len() {
            let end = (at + piece).min(data.len());
            let (count, _, _, _) = self
                .write(handle, offset + at as u64, &data[at..end])
                .unwrap();
            assert!(count > 0);
            at += count as usize;
        }
    }
    pub fn commit(&mut self, handle: &[u8]) -> Result<[u8; 8], u32> {
        let mut arguments = Writer::new();
        arguments.opaque(handle).u64(0).u32(0);
        let (status, rest) = self.nfs(21, &arguments);
        if status != OK {
            return Err(status);
        }
        let mut reader = Reader::new(&rest);
        wcc(&mut reader);
        let mut verifier = [0; 8];
        verifier.copy_from_slice(reader.fixed(8).unwrap());
        Ok(verifier)
    }
    /// REMOVE (`directory` false) or RMDIR.
    pub fn unlink(&mut self, directory: &[u8], name: &[u8], rmdir: bool) -> Result<Wcc, u32> {
        let mut arguments = Writer::new();
        arguments.opaque(directory).opaque(name);
        let (status, rest) = self.nfs(if rmdir { 13 } else { 12 }, &arguments);
        let found = wcc(&mut Reader::new(&rest));
        if status == OK {
            Ok(found)
        } else {
            Err(status)
        }
    }
    pub fn remove(&mut self, directory: &[u8], name: &[u8]) -> Result<Wcc, u32> {
        self.unlink(directory, name, false)
    }
    pub fn rmdir(&mut self, directory: &[u8], name: &[u8]) -> Result<Wcc, u32> {
        self.unlink(directory, name, true)
    }
    pub fn rename(
        &mut self,
        from: &[u8],
        name: &[u8],
        to: &[u8],
        new_name: &[u8],
    ) -> Result<(Wcc, Wcc), u32> {
        let mut arguments = Writer::new();
        arguments
            .opaque(from)
            .opaque(name)
            .opaque(to)
            .opaque(new_name);
        let (status, rest) = self.nfs(14, &arguments);
        let mut reader = Reader::new(&rest);
        let found = (wcc(&mut reader), wcc(&mut reader));
        if status == OK {
            Ok(found)
        } else {
            Err(status)
        }
    }
    pub fn link(&mut self, file: &[u8], directory: &[u8], name: &[u8]) -> Result<Attr, u32> {
        let mut arguments = Writer::new();
        arguments.opaque(file).opaque(directory).opaque(name);
        let (status, rest) = self.nfs(15, &arguments);
        let mut reader = Reader::new(&rest);
        let object = post_op(&mut reader);
        wcc(&mut reader);
        if status == OK {
            Ok(object.expect("LINK returns the file's attributes"))
        } else {
            Err(status)
        }
    }
    /// A path of names below `root`, looked up one by one.
    pub fn walk_to(&mut self, root: &[u8], path: &str) -> Result<(Vec<u8>, Attr), u32> {
        let mut handle = root.to_vec();
        let mut attr = self.getattr(root)?;
        for name in path.split('/').filter(|name| !name.is_empty()) {
            let (next, next_attr) = self.lookup(&handle, name.as_bytes())?;
            handle = next;
            attr = next_attr;
        }
        Ok((handle, attr))
    }
}
