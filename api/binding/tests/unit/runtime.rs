//! Which runtime a deployment selects, and what a local Sandbox record may
//! name. Included by runtime.rs.
use super::{local, native};
use layerfs_bridge::control::SdkValue;

fn deploy(fields: &[(&str, &str)]) -> SdkValue {
    SdkValue::Map(
        (fields.iter())
            .map(|(k, v)| ((*k).to_owned(), SdkValue::text(*v)))
            .collect(),
    )
}

#[test]
fn docker_stays_the_default_and_only_local_selects_local() {
    let image_digest = format!("sha256:{}", "a".repeat(64));
    // unset and `docker` are Docker
    for fields in [vec![], vec![("sandbox", "docker")]] {
        let mut fields = fields;
        fields.push(("image", image_digest.as_str()));
        assert!(!local(&deploy(&fields)).unwrap());
    }
    assert!(local(&deploy(&[("sandbox", "local")])).unwrap());
    // anything else is refused, never taken as Docker or local
    for typo in ["Local", "lokal", "LOCAL", " local", "docker "] {
        let refusal = local(&deploy(&[("sandbox", typo)])).unwrap_err();
        assert_eq!(refusal.code, "InvalidInput");
        assert!(
            refusal.message.contains("docker or local"),
            "{}",
            refusal.message
        );
    }
}

#[test]
fn native_is_a_local_daemon() {
    let native_deploy = deploy(&[("sandbox", "native")]);
    assert!(local(&native_deploy).unwrap());
    assert!(native(&native_deploy));
    assert!(!native(&deploy(&[("sandbox", "local")])));
    assert!(!native(&deploy(&[])));
}

/// A call's fields for Sandbox 1 of project `p` deployed locally under `root`.
#[cfg(target_os = "linux")]
fn call(root: Option<&str>) -> SdkValue {
    let mut fields = vec![("sandbox", "local")];
    if let Some(root) = root {
        fields.push(("localDir", root));
    }
    SdkValue::map([
        ("project", SdkValue::text("p")),
        ("deploy", deploy(&fields)),
    ])
}

#[cfg(target_os = "linux")]
fn record(directory: &str, name: &str) -> SdkValue {
    SdkValue::map([
        ("id", SdkValue::text("1")),
        ("name", SdkValue::text(name)),
        ("container", SdkValue::text("c".repeat(64))),
        ("runtime", SdkValue::text("local")),
        ("directory", SdkValue::text(directory)),
        ("pid", SdkValue::Int(4242)),
        ("start", SdkValue::text("123456")),
        ("boot", SdkValue::text("0123456789abcdef0123456789abcdef")),
        ("port", SdkValue::Int(40404)),
    ])
}

#[cfg(target_os = "linux")]
#[test]
fn a_local_record_is_used_only_where_this_deployment_puts_its_sandbox() {
    use crate::discovery::{recorded, Recorded};
    let args = call(Some("/lfs/local"));
    match recorded(&args, "1", &record("/lfs/local/layerfs-p-1", "layerfs-p-1")) {
        Ok(Recorded::Local {
            directory,
            process,
            port,
            ..
        }) => {
            assert_eq!(directory, std::path::Path::new("/lfs/local/layerfs-p-1"));
            assert_eq!(
                (process.pid, process.start, process.boot, port),
                (
                    4242,
                    123456,
                    Some(0x0123456789abcdef0123456789abcdef),
                    40404
                )
            );
        }
        _ => panic!("a valid local record was refused"),
    }
    for (directory, name) in [
        // a damaged or edited record naming any other directory by its own name
        ("/srv/data", "data"),
        ("/lfs/local/data", "data"),
        // another LAYERFS_LOCAL_DIR, another Sandbox, another name
        ("/old/local/layerfs-p-1", "layerfs-p-1"),
        ("/lfs/local/layerfs-p-2", "layerfs-p-2"),
        ("/lfs/local/layerfs-p-1", "layerfs-p-2"),
        ("/lfs/local/layerfs-p-1/sub", "sub"),
        ("relative/layerfs-p-1", "layerfs-p-1"),
        ("/lfs/../etc/layerfs-p-1", "layerfs-p-1"),
        ("/", ""),
    ] {
        let refusal = match recorded(&args, "1", &record(directory, name)) {
            Err(refusal) => refusal,
            Ok(_) => panic!("{directory} admitted for {name}"),
        };
        assert_eq!(refusal.code, "InvalidInput");
        assert!(
            refusal.message.contains("neither used nor deleted"),
            "{}",
            refusal.message
        );
    }
    // the configured directory is needed to use a local record at all
    for root in [None, Some("relative"), Some("/lfs/../local")] {
        let refusal = match recorded(
            &call(root),
            "1",
            &record("/lfs/local/layerfs-p-1", "layerfs-p-1"),
        ) {
            Err(refusal) => refusal,
            Ok(_) => panic!("{root:?} admitted"),
        };
        assert_eq!(refusal.code, "InvalidInput", "{}", refusal.message);
    }
    // a native record is not used by a local deployment, nor a local one by a native deployment
    let mut native_record = record("/lfs/local/layerfs-p-1", "layerfs-p-1");
    if let SdkValue::Map(fields) = &mut native_record {
        fields.retain(|(k, _)| k != "runtime");
        fields.push(("runtime".into(), SdkValue::text("native")));
    }
    match recorded(&args, "1", &native_record) {
        Err(refusal) => assert!(
            refusal.message.contains("never mixed"),
            "{}",
            refusal.message
        ),
        Ok(_) => panic!("a native record was used by a local deployment"),
    }
    let natively = SdkValue::map([
        ("project", SdkValue::text("p")),
        (
            "deploy",
            deploy(&[("sandbox", "native"), ("localDir", "/lfs/local")]),
        ),
    ]);
    assert!(matches!(
        recorded(&natively, "1", &native_record),
        Ok(Recorded::Local { .. })
    ));
    match recorded(
        &natively,
        "1",
        &record("/lfs/local/layerfs-p-1", "layerfs-p-1"),
    ) {
        Err(refusal) => assert!(
            refusal.message.contains("never mixed"),
            "{}",
            refusal.message
        ),
        Ok(_) => panic!("a local record was used by a native deployment"),
    }
    // a record from before boot ids is read without one; a bad one is refused
    let mut legacy = record("/lfs/local/layerfs-p-1", "layerfs-p-1");
    if let SdkValue::Map(fields) = &mut legacy {
        fields.retain(|(k, _)| k != "boot");
    }
    match recorded(&args, "1", &legacy) {
        Ok(Recorded::Local { process, .. }) => assert_eq!(process.boot, None),
        _ => panic!("a record without a boot id was refused"),
    }
    for (key, value) in [
        ("boot", SdkValue::text("not-hex")),
        ("port", SdkValue::Int(70000)),
    ] {
        let mut bad = record("/lfs/local/layerfs-p-1", "layerfs-p-1");
        if let SdkValue::Map(fields) = &mut bad {
            fields.retain(|(k, _)| k != key);
            fields.push((key.into(), value));
        }
        assert!(recorded(&args, "1", &bad).is_err(), "{key}");
    }
}
