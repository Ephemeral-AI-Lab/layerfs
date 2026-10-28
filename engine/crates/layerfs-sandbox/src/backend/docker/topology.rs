//! Exact observed daemon identity, security and kernel-access settings, and volume placement.
use super::{
    json::{once, Json},
    SandboxRequest,
};
use crate::RuntimeError;
use std::io::Read;
/// The container's whole system-call filter. It denies creation of a user
/// namespace (`CLONE_NEWUSER` to `unshare` or `clone`; `clone3` answers
/// `ENOSYS`, which the C library follows with `clone`) and allows everything
/// else. An unprivileged command can make a mount namespace only inside a
/// user namespace of its own, so with this no command can copy a Workspace
/// mount out of the daemon's namespace. Processes outside the container are
/// not filtered: a copy one makes keeps the connection alive after a plain
/// detach, which the daemon keeps at `Join` for one forced unmount to end.
/// It replaces the Engine's default filter; the command identity holds no
/// capability and, with no user namespace, cannot gain one.
pub(super) const SECCOMP: &str = concat!(
    r#"seccomp={"defaultAction":"SCMP_ACT_ALLOW","syscalls":["#,
    r#"{"names":["unshare","clone"],"action":"SCMP_ACT_ERRNO","errnoRet":1,"#,
    r#""args":[{"index":0,"value":268435456,"valueTwo":268435456,"op":"SCMP_CMP_MASKED_EQ"}]},"#,
    r#"{"names":["clone3"],"action":"SCMP_ACT_ERRNO","errnoRet":38}]}"#
);
/// The daemon's identity and command, and every requested label with its
/// value (the image may add other labels).
pub(super) fn config<R: Read>(
    j: &mut Json<R>,
    labels: &[(String, String)],
) -> Result<bool, RuntimeError> {
    let mut bits = 0;
    let mut valid = true;
    let mut labelled = labels.is_empty();
    j.object(|j, key| {
        if key.equals("Labels") && !labels.is_empty() {
            let mut seen = vec![false; labels.len()];
            j.object(|j, key| {
                match labels.iter().position(|(k, _)| key.equals(k)) {
                    Some(i) => {
                        seen[i] = true;
                        valid &= j.string_equals(&labels[i].1)?;
                    }
                    None => j.skip(3)?,
                }
                Ok(())
            })?;
            labelled = seen.iter().all(|s| *s);
        } else if key.equals("User") {
            once(&mut bits, 1)?;
            valid &= j.string_equals("0:0")?;
        } else if key.equals("Tty") {
            once(&mut bits, 2)?;
            valid &= !j.boolean()?;
        } else if key.equals("Entrypoint") {
            once(&mut bits, 4)?;
            valid &= strings(j, &["/usr/local/bin/layerfs-daemon"])?;
        } else if key.equals("Cmd") {
            once(&mut bits, 8)?;
            valid &= strings(j, &["--config", "/layerfs-local/config/daemon.setup"])?;
        } else {
            j.skip(2)?;
        }
        Ok(())
    })?;
    Ok(bits == 15 && valid && labelled)
}
pub(super) fn host<R: Read>(j: &mut Json<R>) -> Result<bool, RuntimeError> {
    let mut bits = 0;
    let mut valid = true;
    j.object(|j, key| {
        if key.equals("Privileged") {
            once(&mut bits, 1)?;
            valid &= !j.boolean()?;
        } else if key.equals("SecurityOpt") {
            once(&mut bits, 2)?;
            valid &= strings(
                j,
                &["no-new-privileges=true", "apparmor=unconfined", SECCOMP],
            )?;
        } else if key.equals("CapAdd") {
            once(&mut bits, 16)?;
            valid &= strings(j, &["CAP_SYS_ADMIN"])?;
        } else if key.equals("Devices") {
            once(&mut bits, 32)?;
            valid &= fuse_device(j)?;
        } else if key.equals("Init") {
            // the Engine's init reaps what commands leave behind
            once(&mut bits, 64)?;
            valid &= j.boolean()?;
        } else if key.equals("RestartPolicy") {
            once(&mut bits, 4)?;
            let mut seen = 0;
            j.object(|j, key| {
                if key.equals("Name") {
                    once(&mut seen, 1)?;
                    valid &= j.string_equals("no")?;
                } else {
                    j.skip(3)?;
                }
                Ok(())
            })?;
            valid &= seen == 1;
        } else if key.equals("LogConfig") {
            once(&mut bits, 8)?;
            let mut seen = 0;
            j.object(|j, key| {
                if key.equals("Type") {
                    once(&mut seen, 1)?;
                    valid &= j.string_equals("json-file")?;
                } else {
                    j.skip(3)?;
                }
                Ok(())
            })?;
            valid &= seen == 1;
        } else {
            j.skip(2)?;
        }
        Ok(())
    })?;
    Ok(bits == 127 && valid)
}
/// Exactly the one FUSE character device, under its own path.
fn fuse_device<R: Read>(j: &mut Json<R>) -> Result<bool, RuntimeError> {
    let mut count = 0;
    let mut valid = true;
    j.array(|j| {
        count += 1;
        let mut bits = 0;
        j.object(|j, key| {
            if key.equals("PathOnHost") {
                once(&mut bits, 1)?;
                valid &= j.string_equals("/dev/fuse")?;
            } else if key.equals("PathInContainer") {
                once(&mut bits, 2)?;
                valid &= j.string_equals("/dev/fuse")?;
            } else if key.equals("CgroupPermissions") {
                once(&mut bits, 4)?;
                valid &= j.string_equals("rwm")?;
            } else {
                j.skip(3)?;
            }
            Ok(())
        })?;
        valid &= bits == 7;
        Ok(())
    })?;
    Ok(count == 1 && valid)
}
/// Exactly the Store mount, read-write at `/layerfs-store`; any other,
/// missing or repeated mount is a topology mismatch.
pub(super) fn mounts<R: Read>(
    j: &mut Json<R>,
    request: &SandboxRequest,
) -> Result<bool, RuntimeError> {
    let mut count = 0;
    let mut valid = true;
    j.array(|j| {
        count += 1;
        let mut bits = 0;
        let (mut volume, mut name, mut destination, mut rw, mut local) =
            (false, false, false, false, false);
        j.object(|j, key| {
            if key.equals("Type") {
                once(&mut bits, 1)?;
                volume = j.string_equals("volume")?;
            } else if key.equals("Name") {
                once(&mut bits, 2)?;
                name = j.string_equals(&request.store_volume)?;
            } else if key.equals("Destination") {
                once(&mut bits, 4)?;
                destination = j.string_equals("/layerfs-store")?;
            } else if key.equals("RW") {
                once(&mut bits, 8)?;
                rw = j.boolean()?;
            } else if key.equals("Driver") {
                once(&mut bits, 16)?;
                local = j.string_equals("local")?;
            } else {
                j.skip(3)?;
            }
            Ok(())
        })?;
        valid &= bits == 31 && volume && name && destination && rw && local;
        Ok(())
    })?;
    Ok(valid && count == 1)
}
fn strings<R: Read>(j: &mut Json<R>, expected: &[&str]) -> Result<bool, RuntimeError> {
    let mut index = 0;
    let mut valid = true;
    j.array(|j| {
        if let Some(s) = expected.get(index) {
            valid &= j.string_equals(s)?;
        } else {
            valid = false;
            j.skip(3)?;
        }
        index = index.saturating_add(1);
        Ok(())
    })?;
    Ok(index == expected.len() && valid)
}
