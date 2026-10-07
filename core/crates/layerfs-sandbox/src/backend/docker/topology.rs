//! Exact observed daemon identity, security settings and shared-volume placement.
use super::{
    json::{once, Json},
    SandboxRequest,
};
use crate::RuntimeError;
use std::io::Read;
pub(super) fn config<R: Read>(j: &mut Json<R>) -> Result<bool, RuntimeError> {
    let mut bits = 0;
    let mut valid = true;
    j.object(|j, key| {
        if key.equals("User") {
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
    Ok(bits == 15 && valid)
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
            valid &= strings(j, &["no-new-privileges=true"])?;
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
    Ok(bits == 15 && valid)
}
pub(super) fn mounts<R: Read>(
    j: &mut Json<R>,
    request: &SandboxRequest,
) -> Result<bool, RuntimeError> {
    let mut count = 0;
    let mut valid = true;
    j.array(|j| {
        count += 1;
        let mut bits = 0;
        j.object(|j, key| {
            if key.equals("Type") {
                once(&mut bits, 1)?;
                valid &= j.string_equals("volume")?;
            } else if key.equals("Name") {
                once(&mut bits, 2)?;
                valid &= j.string_equals(&request.store_volume)?;
            } else if key.equals("Destination") {
                once(&mut bits, 4)?;
                valid &= j.string_equals("/layerfs-store")?;
            } else if key.equals("RW") {
                once(&mut bits, 8)?;
                valid &= j.boolean()?;
            } else if key.equals("Driver") {
                once(&mut bits, 16)?;
                valid &= j.string_equals("local")?;
            } else {
                j.skip(3)?;
            }
            Ok(())
        })?;
        valid &= bits == 31;
        Ok(())
    })?;
    Ok(count == 1 && valid)
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
