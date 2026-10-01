//! Real native contexts remain uniquely owned through sequential thread moves.
use layerfs_storage::encoding::{CompressionWorkspace, DecompressionWorkspace};
fn require_send<T: Send>() {}
#[test]
fn owned_static_arenas_move_without_alias_or_reallocation() {
    require_send::<CompressionWorkspace>();
    require_send::<DecompressionWorkspace>();
    let first = std::thread::spawn(|| {
        let raw = vec![0x71u8; 4096];
        let mut encode = CompressionWorkspace::new().unwrap();
        let mut decode = DecompressionWorkspace::new().unwrap();
        let frame = encode.compress_group(&raw).unwrap();
        assert_eq!(decode.decompress_group(&frame, raw.len()).unwrap(), raw);
        let sizes = (encode.workspace_bytes(), decode.workspace_bytes());
        (encode, decode, sizes)
    })
    .join()
    .unwrap();
    let second = std::thread::spawn(move || {
        let (mut encode, mut decode, sizes) = first;
        let raw = vec![0x42u8; 8192];
        let frame = encode.compress_group(&raw).unwrap();
        assert_eq!(decode.decompress_group(&frame, raw.len()).unwrap(), raw);
        assert_eq!((encode.workspace_bytes(), decode.workspace_bytes()), sizes);
        (encode, decode, sizes)
    })
    .join()
    .unwrap();
    let (mut encode, mut decode, sizes) = second;
    let raw = vec![0x99u8; 2048];
    let frame = encode.compress_group(&raw).unwrap();
    assert_eq!(decode.decompress_group(&frame, raw.len()).unwrap(), raw);
    assert_eq!((encode.workspace_bytes(), decode.workspace_bytes()), sizes);
}
