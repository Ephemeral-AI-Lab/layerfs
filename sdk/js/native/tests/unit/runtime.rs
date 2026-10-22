//! Which runtime a deployment selects, and what a local Sandbox record may
//! name. Included by runtime.rs.
use super::{local, native, placement};
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

/// A Sandbox record's placement fields: `runtime` (absent on a Docker
/// record) and `image`.
fn placed(runtime: Option<&str>, image: &str) -> SdkValue {
    let mut fields = vec![("image".to_owned(), SdkValue::text(image))];
    if let Some(runtime) = runtime {
        fields.push(("runtime".to_owned(), SdkValue::text(runtime)));
    }
    SdkValue::Map(fields)
}

#[test]
fn placement_keeps_runtimes_apart_even_for_an_image_named_local() {
    let digest = format!("sha256:{}", "a".repeat(64));
    let local_record = placed(Some("local"), "local");
    let docker_record = placed(None, &digest);
    // a Docker deployment whose image is `local`, a valid Docker image name
    let docker_local = SdkValue::map([("deploy", deploy(&[("image", "local")]))]);
    let docker_local = placement(&docker_local).unwrap().unwrap();
    assert!(!docker_local.admits(&local_record));
    assert!(docker_local.admits(&placed(None, "local")));
    // the same image is still the Docker filter, as before
    let docker = SdkValue::map([("deploy", deploy(&[("image", digest.as_str())]))]);
    let docker = placement(&docker).unwrap().unwrap();
    assert!(docker.admits(&docker_record));
    assert!(!docker.admits(&placed(None, &format!("sha256:{}", "b".repeat(64)))));
    assert!(!docker.admits(&local_record));
    // a Docker deployment without an image still never takes a local Sandbox
    let unset = SdkValue::map([("deploy", deploy(&[]))]);
    let unset = placement(&unset).unwrap().unwrap();
    assert!(unset.admits(&docker_record));
    assert!(!unset.admits(&local_record));
    // a local deployment takes only local Sandboxes, whatever image it names
    let local = SdkValue::map([(
        "deploy",
        deploy(&[("sandbox", "local"), ("image", "local")]),
    )]);
    let local = placement(&local).unwrap().unwrap();
    assert!(local.admits(&local_record));
    assert!(!local.admits(&placed(None, "local")));
    assert!(!local.admits(&docker_record));
    // no deployment: no filter, as before
    assert!(
        placement(&SdkValue::map([("project", SdkValue::text("p"))]))
            .unwrap()
            .is_none()
    );
}

#[test]
fn native_is_a_local_daemon_placed_apart_from_local_and_docker() {
    let native_deploy = deploy(&[("sandbox", "native")]);
    assert!(local(&native_deploy).unwrap());
    assert!(native(&native_deploy));
    assert!(!native(&deploy(&[("sandbox", "local")])));
    assert!(!native(&deploy(&[])));
    let native_record = placed(Some("native"), "local");
    let local_record = placed(Some("local"), "local");
    let docker_record = placed(None, &format!("sha256:{}", "a".repeat(64)));
    let natively = SdkValue::map([("deploy", native_deploy)]);
    let natively = placement(&natively).unwrap().unwrap();
    assert!(natively.admits(&native_record));
    assert!(!natively.admits(&local_record));
    assert!(!natively.admits(&docker_record));
    let locally = SdkValue::map([("deploy", deploy(&[("sandbox", "local")]))]);
    let locally = placement(&locally).unwrap().unwrap();
    assert!(!locally.admits(&native_record));
    let docker = SdkValue::map([("deploy", deploy(&[]))]);
    assert!(!placement(&docker).unwrap().unwrap().admits(&native_record));
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
