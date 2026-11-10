//! XDR, RPC record marking, call headers and file handles through the public
//! API, and the export's transfer window: 256 KiB per call, one engine write
//! window and two bounded read windows.
#[allow(dead_code)]
#[path = "support/client.rs"]
mod client;
#[allow(dead_code)]
#[path = "support/rig.rs"]
mod rig;
#[allow(dead_code)]
#[path = "support/spy.rs"]
mod spy;
use client::{Client, OK};
use layerfs_nfs::{
    handle::{HandleError, SqliteHandles, HANDLE_BYTES},
    rpc::{parse_call, write_record, Records, Refused, RECORD_LIMIT},
    xdr::{opaque_size, Reader, Writer},
    TRANSFER_WINDOW,
};
use std::{
    fs,
    io::{self, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    sync::{atomic::Ordering, Arc},
    time::Duration,
};

const UID: u32 = 501;
const GID: u32 = 20;
/// The export's one transfer window, as bytes: one WRITE call, one job.
const WINDOW: usize = TRANSFER_WINDOW as usize;
const FILE_SYNC: u32 = 2;
// One transfer is a whole number of engine read windows and fits one RPC
// record, and the claims of one write window still cover its smallest chunks.
const _: () = assert!(TRANSFER_WINDOW as usize % layerfs_overlay::READ_WINDOW == 0);
const _: () = assert!(RECORD_LIMIT as u32 >= TRANSFER_WINDOW + 4096);
const _: () = assert!(layerfs_overlay::WRITE_CLAIMS * 8_192 >= layerfs_overlay::WRITE_WINDOW);

#[test]
fn xdr_round_trips_with_padding() {
    let mut out = Writer::new();
    out.u32(7)
        .u64(u64::MAX - 1)
        .bool(true)
        .opaque(b"abcde")
        .fixed(b"xy");
    assert_eq!(out.len(), 4 + 8 + 4 + opaque_size(5) + 4);
    let bytes = out.into_bytes();
    let mut reader = Reader::new(&bytes);
    assert_eq!(reader.u32(), Ok(7));
    assert_eq!(reader.u64(), Ok(u64::MAX - 1));
    assert_eq!(reader.bool(), Ok(true));
    assert_eq!(reader.opaque(5), Ok(&b"abcde"[..]));
    assert_eq!(reader.fixed(2), Ok(&b"xy"[..]));
    assert_eq!(reader.remaining(), 0);
    assert!(reader.u32().is_err(), "reads past the record fail");
    let mut long = Writer::new();
    long.opaque(&[0; 9]);
    let long = long.into_bytes();
    assert!(
        Reader::new(&long).opaque(8).is_err(),
        "a length beyond its bound"
    );
    assert!(
        Reader::new(&[0, 0, 0, 2]).bool().is_err(),
        "a bool is 0 or 1"
    );
}

#[test]
fn records_join_fragments_and_respect_their_bound() {
    let mut wire = Vec::new();
    wire.extend_from_slice(&3u32.to_be_bytes());
    wire.extend_from_slice(b"abc");
    wire.extend_from_slice(&(2u32 | (1 << 31)).to_be_bytes());
    wire.extend_from_slice(b"de");
    write_record(&mut wire, b"next").unwrap();
    let mut stream = &wire[..];
    let mut records = Records::new();
    assert_eq!(records.read(&mut stream).unwrap(), Some(b"abcde".to_vec()));
    assert_eq!(records.read(&mut stream).unwrap(), Some(b"next".to_vec()));
    assert_eq!(records.read(&mut stream).unwrap(), None, "orderly end");
    let huge = (RECORD_LIMIT as u32 + 1) | (1 << 31);
    assert!(Records::new().read(&mut &huge.to_be_bytes()[..]).is_err());
}
/// A stream that accepts at most `step` bytes of each write and says so,
/// as a socket with a full send buffer does.
struct Trickle {
    bytes: Vec<u8>,
    step: usize,
}
impl Write for Trickle {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let taken = buffer.len().min(self.step);
        self.bytes.extend_from_slice(&buffer[..taken]);
        Ok(taken)
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn a_framed_reply_reaches_a_stream_that_takes_it_in_pieces() {
    // The header and the reply are offered as two slices of one vectored
    // write; a stream that accepts less continues where it stopped, and the
    // reply is never copied to be framed.
    let reply: Vec<u8> = (0..4093).map(|at| at as u8).collect();
    for step in [1, 3, 4, 511, 4096] {
        let mut stream = Trickle {
            bytes: Vec::new(),
            step,
        };
        write_record(&mut stream, &reply).unwrap();
        let mut wire = &stream.bytes[..];
        let mut records = Records::new();
        assert_eq!(
            records.read(&mut wire).unwrap(),
            Some(reply.clone()),
            "{step}"
        );
        assert_eq!(records.read(&mut wire).unwrap(), None, "{step}");
    }
}

fn call(rpcvers: u32, flavor: u32, credential: &[u8]) -> Vec<u8> {
    let mut out = Writer::new();
    out.u32(9).u32(0).u32(rpcvers).u32(100_003).u32(3).u32(1);
    out.u32(flavor).opaque(credential).u32(0).u32(0);
    out.u32(0xabcd);
    out.into_bytes()
}

#[test]
fn call_headers_carry_unix_credentials() {
    let mut unix = Writer::new();
    unix.u32(0)
        .opaque(b"host")
        .u32(501)
        .u32(20)
        .u32(2)
        .u32(12)
        .u32(61);
    let record = call(2, 1, unix.as_bytes());
    let (header, mut arguments) = parse_call(&record).unwrap();
    assert_eq!(
        (header.xid, header.program, header.version, header.procedure),
        (9, 100_003, 3, 1)
    );
    let credentials = header.credentials.unix.unwrap();
    assert_eq!(
        (credentials.uid, credentials.gid, credentials.groups),
        (501, 20, vec![12, 61])
    );
    assert_eq!(arguments.u32(), Ok(0xabcd));
    assert!(parse_call(&call(2, 0, b""))
        .unwrap()
        .0
        .credentials
        .unix
        .is_none());
    // RPC version 3 and an unknown flavor are answered with a denial.
    for record in [call(3, 0, b""), call(2, 6, b"")] {
        match parse_call(&record) {
            Err(Refused::Reply(reply)) => {
                assert_eq!(&reply[..12], &[0, 0, 0, 9, 0, 0, 0, 1, 0, 0, 0, 1])
            }
            Err(Refused::Undecodable) => panic!("undecodable"),
            Ok((call, _)) => panic!("{call:?}"),
        }
    }
    assert!(matches!(
        parse_call(&[0, 0, 0, 9, 0, 0, 0, 1]),
        Err(Refused::Undecodable)
    ));
}

#[test]
fn handles_are_stable_per_mount_and_stale_after_it() {
    let handles = SqliteHandles::new(41);
    let first = handles.encode(1234);
    assert_eq!(first.len(), HANDLE_BYTES);
    assert_eq!(
        handles.encode(1234),
        first,
        "the same serial, the same bytes"
    );
    assert_eq!(handles.decode(&first), Ok(1234));
    let mut forged = first;
    forged[16] ^= 1;
    assert_eq!(handles.decode(&forged), Err(HandleError::Bad));
    assert_eq!(handles.decode(&first[..24]), Err(HandleError::Bad));
    assert_eq!(
        SqliteHandles::new(42).decode(&first),
        Err(HandleError::Stale)
    );
}
fn temp(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("layerfs-nfs-p{}-{label}", std::process::id()))
}
fn source(root: &Path) {
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("README.md"), b"# fixture\n").unwrap();
    fs::write(root.join("src/lib.rs"), b"pub fn answer() -> u32 { 42 }\n").unwrap();
}
fn pattern(seed: u32, length: usize) -> Vec<u8> {
    (0..length)
        .map(|n| (n as u32).wrapping_mul(31).wrapping_add(seed * 97) as u8)
        .collect()
}
/// One `fattr3`, skipped field by field.
fn attr(reader: &mut Reader<'_>) {
    for _ in 0..5 {
        reader.u32().unwrap();
    }
    for _ in 0..2 {
        reader.u64().unwrap();
    }
    for _ in 0..2 {
        reader.u32().unwrap();
    }
    for _ in 0..2 {
        reader.u64().unwrap();
    }
    for _ in 0..6 {
        reader.u32().unwrap();
    }
}
/// The FSINFO answer for `handle`: (rtmax, rtpref, wtmax, wtpref, dtpref,
/// properties); `rtmult`, `wtmult`, `maxfilesize` and `time_delta` are checked
/// here.
fn fsinfo(c: &mut Client, handle: &[u8]) -> (u32, u32, u32, u32, u32, u32) {
    let mut arguments = Writer::new();
    arguments.opaque(handle);
    let (status, rest) = c.nfs(19, &arguments);
    assert_eq!(status, OK);
    let mut reader = Reader::new(&rest);
    assert!(reader.bool().unwrap(), "the object's attributes");
    attr(&mut reader);
    let read = (
        reader.u32().unwrap(),
        reader.u32().unwrap(),
        reader.u32().unwrap(),
    );
    let write = (
        reader.u32().unwrap(),
        reader.u32().unwrap(),
        reader.u32().unwrap(),
    );
    let preference = reader.u32().unwrap();
    let _maxfilesize = reader.u64().unwrap();
    assert_eq!(
        (reader.u32().unwrap(), reader.u32().unwrap()),
        (0, 1),
        "time_delta: 1 ns"
    );
    let properties = reader.u32().unwrap();
    assert_eq!(reader.remaining(), 0, "FSINFO results decoded");
    assert_eq!((read.2, write.2), (4096, 4096), "block sizes");
    (read.0, read.1, write.0, write.1, preference, properties)
}

#[test]
fn fsinfo_declares_the_transfer_window_over_bounded_engine_windows() {
    assert_eq!(TRANSFER_WINDOW, 256 * 1024);
    assert_eq!(
        TRANSFER_WINDOW as usize,
        layerfs_overlay::WRITE_WINDOW,
        "one call is one engine write window"
    );

    let base = temp("fsinfo");
    let tree = base.join("tree");
    source(&tree);
    let rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    let server = rig.serve(TcpListener::bind("127.0.0.1:0").unwrap(), UID, GID);
    let mut c = Client::connect(server.local_addr(), UID, GID);
    let root = c.mnt("/");
    let (rtmax, rtpref, wtmax, wtpref, dtpref, properties) = fsinfo(&mut c, &root);
    assert_eq!((rtmax, rtpref), (TRANSFER_WINDOW, TRANSFER_WINDOW));
    assert_eq!((wtmax, wtpref), (TRANSFER_WINDOW, TRANSFER_WINDOW));
    assert_eq!(dtpref, 32 * 1024);
    assert_eq!(properties, 0x01 | 0x02 | 0x08 | 0x10);
    drop(c);
    let released = server.stop();
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0);
    fs::remove_dir_all(base).unwrap();
}

#[test]
fn a_whole_transfer_window_write_and_read_travel_in_one_call() {
    let base = temp("transfer");
    let tree = base.join("tree");
    source(&tree);
    let rig = rig::Rig::new(&tree, &base.join("rig"), Duration::from_secs(30));
    let watch = Arc::new(spy::Watch::default());
    let services = Arc::new(spy::Spy {
        inner: Arc::new(layerfs_daemon::Mounted(rig.bound.clone())),
        watch: watch.clone(),
    });
    let server = rig.export_services(
        TcpListener::bind("127.0.0.1:0").unwrap(),
        services,
        UID,
        GID,
        false,
    );
    let mut c = Client::connect(server.local_addr(), UID, GID);
    c.wire = Some(watch.wire.clone());
    let root = c.mnt("/");

    let made = c.touch(&root, b"big.bin", 0o644).unwrap();
    let created = watch.published.load(Ordering::Acquire);
    assert_eq!(created, 1, "CREATE is one publication");

    // A whole transfer window in one WRITE call at a fresh file's end: the
    // call is one engine window, so it is one publication as well.
    let data = pattern(3, WINDOW);
    let calls = c.calls;
    let (count, committed, _, file) = c.write(&made.handle, 0, &data).unwrap();
    assert_eq!(c.calls, calls + 1, "one transfer WRITE is one call");
    assert_eq!(count as usize, WINDOW);
    assert_eq!(committed, FILE_SYNC);
    assert_eq!(file.after.unwrap().size, WINDOW as u64);
    assert_eq!(
        watch.published.load(Ordering::Acquire) - created,
        WINDOW.div_ceil(layerfs_overlay::WRITE_WINDOW) as u64,
        "one publication per engine window"
    );

    // A window that starts inside a cell and ends short of one: it crosses a
    // cell and a slot boundary and appends a ragged tail.
    let tail = pattern(4, 4096 + 1000 + 7);
    let (count, _, _, file) = c
        .write(&made.handle, (WINDOW - 1000) as u64, &tail)
        .unwrap();
    assert_eq!(count as usize, tail.len());
    let size = WINDOW + tail.len() - 1000;
    assert_eq!(file.after.unwrap().size, size as u64);

    // One READ call asks for two transfer windows and gets one: it is filled
    // from consecutive engine read windows, each of the bounded size.
    let seen = watch.reads.lock().unwrap().len();
    let calls = c.calls;
    let (read, eof) = c.read(&made.handle, 0, 2 * WINDOW as u32).unwrap();
    assert_eq!(c.calls, calls + 1, "one READ fills the whole transfer");
    assert_eq!(read.len(), TRANSFER_WINDOW as usize);
    assert!(!eof, "the file continues past one transfer");
    // The tail window overwrote the first window's last 1000 bytes.
    assert_eq!(&read[..WINDOW - 1000], &data[..WINDOW - 1000]);
    assert_eq!(&read[WINDOW - 1000..WINDOW], &tail[..1000]);
    let reads = watch.reads.lock().unwrap()[seen..].to_vec();
    assert_eq!(
        reads.len(),
        WINDOW.div_ceil(layerfs_overlay::READ_WINDOW),
        "one engine window per read window"
    );
    assert!(
        reads
            .iter()
            .all(|(_, length)| *length as usize <= layerfs_overlay::READ_WINDOW),
        "engine read windows stay bounded: {reads:?}"
    );
    assert_eq!(reads[0].0, 0);
    assert_eq!(reads[1].0, layerfs_overlay::READ_WINDOW as u64);

    // The window after the transfer holds the ragged tail and ends the file.
    let (rest, eof) = c
        .read(&made.handle, TRANSFER_WINDOW as u64, WINDOW as u32)
        .unwrap();
    assert_eq!(rest, &tail[1000..]);
    assert!(eof, "a short last window ends the file");

    // A window that begins inside a cell returns the same bytes.
    let (window, eof) = c
        .read(&made.handle, (WINDOW - 1000) as u64, 4 * 1024)
        .unwrap();
    assert_eq!(window, &tail[..4 * 1024]);
    assert!(!eof);

    // A READ at or past the end is an empty, final answer.
    let (empty, eof) = c.read(&made.handle, size as u64, 4096).unwrap();
    assert!(empty.is_empty() && eof);

    drop(c);
    let released = server.stop();
    let (_, outstanding) = rig.stop(released);
    assert_eq!(outstanding, 0);
    fs::remove_dir_all(base).unwrap();
}
