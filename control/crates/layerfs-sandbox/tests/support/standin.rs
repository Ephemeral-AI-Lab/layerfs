//! A stand-in `docker` CLI for image copy tests: a shell script answering the
//! few commands the copy runs from files in its own directory, so no test
//! needs an Engine.
use layerfs_sandbox::docker::Docker;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

const SCRIPT: &str = r#"#!/bin/sh
D="$(dirname "$0")"
if [ "$1" = "-H" ]; then shift 2; fi
key() { printf '%s' "$1" | tr '/:' '__'; }
case "$1 $2" in
  "image inspect")
    shift 4; missing=0
    for ref in "$@"; do
      f="$D/images/$(key "$ref").json"
      if [ -f "$f" ]; then cat "$f"; echo; else echo "Error: No such image: $ref" >&2; missing=1; fi
    done
    exit $missing ;;
  "image ls") cat "$D/ls.txt" 2>/dev/null; exit 0 ;;
  "container inspect")
    if [ "$3" = "--size" ]; then cat "$D/rootfs_size"; exit 0; fi
    if [ -f "$D/containers/$5" ]; then echo id; exit 0; fi
    echo "Error: No such object: $5" >&2; exit 1 ;;
esac
case "$1" in
  create) mkdir -p "$D/containers"; touch "$D/containers/$3"; echo id ;;
  export) echo x >> "$D/exports"; cat "$D/export.tar" ;;
  rm) rm -f "$D/containers/$3" ;;
  ps) ;;
  *) echo "unexpected: $*" >&2; exit 2 ;;
esac
"#;

/// The stand-in's directory and the Docker that runs it.
pub struct Standin {
    pub dir: PathBuf,
}

impl Standin {
    pub fn new(dir: &Path) -> Self {
        fs::create_dir_all(dir.join("images")).unwrap();
        let program = dir.join("docker");
        fs::write(&program, SCRIPT).unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(dir.join("rootfs_size"), "0\n").unwrap();
        Self {
            dir: dir.to_owned(),
        }
    }

    pub fn docker(&self) -> Docker {
        Docker::at(Some(self.dir.join("docker")), None)
    }

    /// Docker holds image `id` as `refs`, for `architecture`, of `size`.
    pub fn image(&self, id: &str, refs: &[&str], (os, architecture): (&str, &str), size: u64) {
        let tags = refs
            .iter()
            .map(|r| format!("\"{r}\""))
            .collect::<Vec<_>>()
            .join(",");
        let json = format!(
            r#"{{"Id":"{id}","RepoTags":[{tags}],"Os":"{os}","Architecture":"{architecture}","Size":{size},"Config":{{"Env":["PATH=/usr/bin"],"User":""}}}}"#
        );
        for r in refs.iter().copied().chain([id]) {
            let key = r.replace(['/', ':'], "_");
            fs::write(self.dir.join("images").join(format!("{key}.json")), &json).unwrap();
        }
        let mut ls = fs::read_to_string(self.dir.join("ls.txt")).unwrap_or_default();
        for r in refs {
            ls.push_str(r);
            ls.push('\n');
        }
        fs::write(self.dir.join("ls.txt"), ls).unwrap();
    }

    /// `docker export` answers these bytes.
    pub fn export(&self, bytes: &[u8]) {
        fs::write(self.dir.join("export.tar"), bytes).unwrap();
    }

    /// How many times `docker export` ran.
    pub fn exports(&self) -> usize {
        fs::read_to_string(self.dir.join("exports"))
            .map(|t| t.lines().count())
            .unwrap_or(0)
    }

    pub fn set_rootfs_size(&self, size: u64) {
        fs::write(self.dir.join("rootfs_size"), format!("{size}\n")).unwrap();
    }
}
