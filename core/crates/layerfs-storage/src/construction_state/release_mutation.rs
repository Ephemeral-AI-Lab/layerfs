//! Full before/proposed frontier CAS retained across one commit observation.
use super::{count_index, profile, release_index, release_state::Release};
use crate::{StorageError, StorageResult};
use rusqlite::Connection;
pub(crate) fn commit(
    c: &Connection,
    engine: Option<&'static crate::engine::EngineGuard>,
    state: &Release,
) -> StorageResult<()> {
    if let Some(guard) = engine {
        guard.validate()?;
    }
    crate::sqlite::write::begin_immediate(c)?;
    let result = (|| {
        release_index::verify(c, state)?;
        let a = state.attempt.as_ref().unwrap();
        for j in &a.jobs {
            if c.execute(
                "INSERT INTO release_jobs VALUES(?1,?2)",
                rusqlite::params![
                    state.scope.key(j.sequence)?.as_slice(),
                    j.encode_value()?.as_slice()
                ],
            )? != 1
            {
                return Err(StorageError::Integrity("release job acknowledgement"));
            }
        }
        if let Some(j) = a.taken {
            if c.execute(
                "DELETE FROM release_jobs WHERE key=?1 AND value=?2",
                rusqlite::params![
                    state.scope.key(j.sequence)?.as_slice(),
                    j.encode_value()?.as_slice()
                ],
            )? != 1
            {
                return Err(StorageError::Integrity("release take acknowledgement"));
            }
        }
        if let Some((old, new)) = &a.frame {
            let scope = state.scope.frames()?;
            let n = match (old, new) {
                (None, Some(f)) => c.execute(
                    "INSERT INTO release_frames VALUES(?1,?2)",
                    rusqlite::params![scope.key(f.depth)?.as_slice(), f.encode_value().as_slice()],
                )?,
                (Some(old), Some(new)) => c.execute(
                    "UPDATE release_frames SET value=?1 WHERE key=?2 AND value=?3",
                    rusqlite::params![
                        new.encode_value().as_slice(),
                        scope.key(old.depth)?.as_slice(),
                        old.encode_value().as_slice()
                    ],
                )?,
                (Some(f), None) => c.execute(
                    "DELETE FROM release_frames WHERE key=?1 AND value=?2",
                    rusqlite::params![scope.key(f.depth)?.as_slice(), f.encode_value().as_slice()],
                )?,
                _ => return Err(StorageError::Integrity("release frame mutation kind")),
            };
            if n != 1 {
                return Err(StorageError::Integrity("release frame acknowledgement"));
            }
        }
        release_index::write(c, &state.scope, &a.after)?;
        if a.after.stage >= 4
            && (!count_index::empty(c, "release_jobs")?
                || !count_index::empty(c, "release_frames")?
                || a.after.current.is_some())
        {
            return Err(StorageError::Integrity("release terminal exact EOF"));
        }
        Ok(())
    })();
    profile::finish_write_guarded(c, result, engine)
}
