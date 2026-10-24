//! XDR, RPC record marking, call headers and file handles through the public API.
use layerfs_nfs::{
    handle::{HandleError, Handles, HANDLE_BYTES},
    rpc::{parse_call, read_record, write_record, Refused, RECORD_LIMIT},
    xdr::{opaque_size, Reader, Writer},
};

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
    assert_eq!(read_record(&mut stream).unwrap(), Some(b"abcde".to_vec()));
    assert_eq!(read_record(&mut stream).unwrap(), Some(b"next".to_vec()));
    assert_eq!(read_record(&mut stream).unwrap(), None, "orderly end");
    let huge = (RECORD_LIMIT as u32 + 1) | (1 << 31);
    assert!(read_record(&mut &huge.to_be_bytes()[..]).is_err());
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
    let handles = Handles::new(41);
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
    assert_eq!(Handles::new(42).decode(&first), Err(HandleError::Stale));
}
