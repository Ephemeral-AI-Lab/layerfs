use phase6_live_probe::scenario::Scenario;
fn file(body: &[u8]) -> std::path::PathBuf {
    let p = std::env::temp_dir().join(format!(
        "p6-scenario-{}",
        phase6_live_probe::minio::hex(&layerfs_sandbox::random::<16>().unwrap())
    ));
    std::fs::write(&p, body).unwrap();
    p
}
fn blob(out: &mut Vec<u8>, text: &str) {
    out.extend_from_slice(&(text.len() as u32).to_be_bytes());
    out.extend_from_slice(text.as_bytes());
}
fn body(manifest: &str) -> Vec<u8> {
    let mut b = b"P6CASE1\0".to_vec();
    blob(&mut b, "fixture");
    b.extend_from_slice(&1u32.to_be_bytes());
    blob(&mut b, "arbitrary-command");
    blob(&mut b, "printf literal > file");
    blob(&mut b, manifest);
    b
}
#[test]
fn sealed_input_exact_eof_and_manifest_refusal() {
    let p = file(&body(".\td\t493\t0\t-\n"));
    let s = Scenario::load(&p).unwrap();
    assert_eq!(s.steps[0].command, "printf literal > file");
    std::fs::remove_file(p).unwrap();
    let mut trailing = body(".\td\t493\t0\t-\n");
    trailing.push(0);
    let p = file(&trailing);
    assert!(Scenario::load(&p).unwrap_err_text().contains("trailing"));
    std::fs::remove_file(p).unwrap();
    let p = file(&body(".\td\t493\t0\t-\n.\td\t493\t0\t-\n"));
    assert!(Scenario::load(&p).is_err());
    std::fs::remove_file(p).unwrap();
}
trait ErrorText {
    fn unwrap_err_text(self) -> String;
}
impl ErrorText for Result<Scenario, String> {
    fn unwrap_err_text(self) -> String {
        match self {
            Err(e) => e,
            Ok(_) => panic!("unexpected accepted input"),
        }
    }
}
