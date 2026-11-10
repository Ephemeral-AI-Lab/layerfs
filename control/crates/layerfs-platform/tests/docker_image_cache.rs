//! Engine-owned content tags must reuse images across homes and runtime builds.
#![cfg(any(target_os = "linux", target_os = "macos"))]
#[path = "support/standin.rs"]
mod standin;
use layerfs_platform::{self as machine, Code, Platform};
use standin::Scratch;

fn provider(scratch: &Scratch) {
    scratch.tool(
        "docker",
        r#"case "$3 $4" in
  "image inspect")
    [ -e "$SCRATCH/engine-failed" ] && { echo 'connection refused' >&2; exit 1; }
    [ -e "$SCRATCH/tagged" ] && { printf 'sha256:%064d\n' 7; exit 0; }
    echo 'Error response from daemon: No such image' >&2; exit 1 ;;
  "load -i")
    [ -e "$SCRATCH/load-failed" ] && { echo 'invalid tar' >&2; exit 1; }
    printf 'Loaded image ID: sha256:%064d\n' 7 ;;
  "image tag") touch "$SCRATCH/tagged" ;;
esac"#,
    );
}

#[test]
fn content_tag_is_shared_and_missing_image_is_imported_only_once() {
    let s = Scratch::new("image-cache");
    provider(&s);
    let archive = s.path("sandbox-image.tar");
    std::fs::write(&archive, b"archive").unwrap();
    let content = format!("sha256:{}", "ab".repeat(32));
    let mut spec = s.spec(Platform::Docker);
    let first = machine::cached_image(&spec, &archive, &content, None).unwrap();
    spec.name = "another-home".into();
    let second = machine::cached_image(&spec, &archive, &content, None).unwrap();
    assert_eq!(first, second);
    assert_eq!(s.log().matches(" load -i ").count(), 1, "{}", s.log());
    assert!(s
        .log()
        .contains(&format!("layerfs-cache/sandbox:{}", "ab".repeat(32))));
    // An externally pruned image is a cache miss, not a stale recorded ID.
    std::fs::remove_file(s.path("tagged")).unwrap();
    machine::cached_image(&spec, &archive, &content, None).unwrap();
    assert_eq!(s.log().matches(" load -i ").count(), 2);
}

#[test]
fn engine_failure_and_failed_import_are_never_replayed() {
    for failure in ["engine-failed", "load-failed"] {
        let s = Scratch::new(failure);
        provider(&s);
        std::fs::write(s.path(failure), b"").unwrap();
        let content = format!("sha256:{}", "ab".repeat(32));
        let error =
            machine::cached_image(&s.spec(Platform::Docker), &s.path("a.tar"), &content, None)
                .unwrap_err();
        assert_eq!(error.code, Code::Failed);
        let loads = s.log().matches(" load -i ").count();
        assert_eq!(loads, usize::from(failure == "load-failed"));
        assert!(!s.log().contains("image tag"));
    }
}

#[test]
fn invalid_content_is_refused_before_any_docker_command() {
    let s = Scratch::new("image-invalid");
    provider(&s);
    let error = machine::cached_image(&s.spec(Platform::Docker), &s.path("a.tar"), "../bad", None)
        .unwrap_err();
    assert_eq!(error.code, Code::InvalidInput);
    assert!(s.log().is_empty());
}
