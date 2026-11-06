//! `lima` template: see `mod.rs`.
use super::observe::disk_name;
use crate::setup::vm::mib;
use crate::spec::{Spec, WORKER_PORT};

const UBUNTU: &str =
    "https://cloud-images.ubuntu.com/releases/24.04/release/ubuntu-24.04-server-cloudimg";
/// The instance's template, as JSON (which is YAML).
pub(crate) fn template(spec: &Spec, port: u16) -> String {
    let images = match spec.os_image.as_deref() {
        Some(image) if image.starts_with("https://") || image.starts_with('/') => {
            format!("[{{\"location\": {}}}]", quote(image))
        }
        _ => format!(
            "[{{\"location\": \"{UBUNTU}-arm64.img\", \"arch\": \"aarch64\"}}, \
             {{\"location\": \"{UBUNTU}-amd64.img\", \"arch\": \"x86_64\"}}]"
        ),
    };
    let nested = if spec.nested {
        "\n  \"nestedVirtualization\": true,"
    } else {
        ""
    };
    format!(
        r#"{{
  "vmType": "vz",{nested}
  "cpus": {cpus},
  "memory": "{memory}iB",
  "images": {images},
  "containerd": {{"system": false, "user": false}},
  "mounts": [],
  "additionalDisks": [{{"name": "{disk}", "format": true, "fsType": "ext4"}}],
  "portForwards": [
    {{"guestIP": "127.0.0.1", "guestPort": {WORKER_PORT}, "hostIP": "127.0.0.1", "hostPort": {port}}},
    {{"guestIP": "0.0.0.0", "guestPortRange": [1, 65535], "ignore": true}}
  ]
}}
"#,
        cpus = spec.cpus,
        memory = mib(spec.memory),
        disk = disk_name(&spec.vm_name),
    )
}

fn quote(text: &str) -> String {
    let mut out = Vec::new();
    layerfs_sandbox_backend::json::quoted(&mut out, text).expect("writing to a Vec");
    String::from_utf8(out).expect("JSON text is UTF-8")
}
