use layerfs_storage_core::{read_frame, write_frame, Frame, FrameKind};

#[test]
fn round_trip_and_incomplete_rejection() {
    let frame = Frame {
        kind: FrameKind::Payload,
        bytes: vec![7; 4096],
    };
    let mut encoded = Vec::new();
    write_frame(&mut encoded, &frame).unwrap();
    assert_eq!(read_frame(&mut encoded.as_slice()).unwrap(), frame);
    assert!(read_frame(&mut encoded[..encoded.len() - 1].as_ref()).is_err());
    encoded[20] ^= 1;
    assert!(read_frame(&mut encoded.as_slice()).is_err());
}
