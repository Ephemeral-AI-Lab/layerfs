use super::*;

#[test]
fn full_create_cleanup_preserves_a_substituted_path() {
    let path = test_path();
    let displaced = test_path();
    let replacement = b"not the create-owned file";
    let result = FullStorage::create_durable_with_injector(&path, &mut |point| {
        fs::rename(&path, &displaced).unwrap();
        fs::write(&path, replacement).unwrap();
        Err(EngineError::InjectedFailure(point))
    });
    assert!(matches!(
        result,
        Err(EngineError::InjectedFailure("file_created"))
    ));
    assert_eq!(fs::read(&path).unwrap(), replacement);
    assert_eq!(fs::metadata(&displaced).unwrap().len(), 0);
    fs::remove_file(path).unwrap();
    fs::remove_file(displaced).unwrap();
}
