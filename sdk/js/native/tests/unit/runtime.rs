//! Which runtime a deployment selects, and what a local Sandbox record may
//! name. Included by runtime.rs.
use super::{image, local, LOCAL};
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
    // unset and `docker` are Docker, placed by the deployment's image as before
    for fields in [vec![], vec![("sandbox", "docker")]] {
        let mut fields = fields;
        fields.push(("image", image_digest.as_str()));
        let deploy = deploy(&fields);
        assert!(!local(&deploy).unwrap());
        let args = SdkValue::map([("deploy", deploy)]);
        assert_eq!(
            image(&args).unwrap().as_deref(),
            Some(image_digest.as_str())
        );
    }
    // no deployment: no image filter, as before
    assert_eq!(
        image(&SdkValue::map([("project", SdkValue::text("p"))])).unwrap(),
        None
    );
    // local ignores any image: its Sandboxes are placed as `local`
    let deploy_local = deploy(&[("sandbox", "local"), ("image", image_digest.as_str())]);
    assert!(local(&deploy_local).unwrap());
    let args = SdkValue::map([("deploy", deploy_local)]);
    assert_eq!(image(&args).unwrap().as_deref(), Some(LOCAL));
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

fn record(directory: &str, name: &str) -> SdkValue {
    SdkValue::map([
        ("id", SdkValue::text("1")),
        ("name", SdkValue::text(name)),
        ("container", SdkValue::text("c".repeat(64))),
        ("runtime", SdkValue::text("local")),
        ("directory", SdkValue::text(directory)),
        ("pid", SdkValue::Int(4242)),
        ("start", SdkValue::text("123456")),
        ("port", SdkValue::Int(40404)),
    ])
}

#[test]
fn a_local_record_names_its_own_directory_only() {
    match crate::discovery::recorded(&record("/lfs/local/layerfs-p-1", "layerfs-p-1")) {
        Ok(crate::discovery::Recorded::Local {
            directory,
            process,
            port,
            ..
        }) => {
            assert_eq!(directory, std::path::Path::new("/lfs/local/layerfs-p-1"));
            assert_eq!((process.pid, process.start, port), (4242, 123456, 40404));
        }
        _ => panic!("a valid local record was refused"),
    }
    for (directory, name) in [
        ("relative/layerfs-p-1", "layerfs-p-1"),
        ("/lfs/local/other", "layerfs-p-1"),
        ("/lfs/../etc/layerfs-p-1", "layerfs-p-1"),
        ("/", ""),
    ] {
        assert!(
            crate::discovery::recorded(&record(directory, name)).is_err(),
            "{directory} admitted for {name}"
        );
    }
    let mut bad_port = record("/lfs/local/layerfs-p-1", "layerfs-p-1");
    if let SdkValue::Map(fields) = &mut bad_port {
        fields.retain(|(k, _)| k != "port");
        fields.push(("port".into(), SdkValue::Int(70000)));
    }
    assert!(crate::discovery::recorded(&bad_port).is_err());
}
