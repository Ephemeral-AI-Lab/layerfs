use super::*;
use std::time::Instant;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RefState {
    pub name: String,
    pub generation: u64,
    pub root: ObjectId,
}

impl Engine {
    pub fn read_ref(&self, name: &str) -> EngineResult<Option<RefState>> {
        validate_ref_name(name)?;
        let connection = self.lock_connection()?;
        let query_started = Instant::now();
        self.mark_statement()?;
        let result = read_ref_on_connection(&connection, name);
        observe_time(&self.timings.nonpayload_query_ns, query_started);
        result
    }

    pub fn fork_ref(&self, source: &RefState, new_name: &str) -> EngineResult<RefState> {
        validate_ref_name(new_name)?;
        let mut publication = self.begin_publication(None, new_name)?;
        publication.retain_existing_root(source.root)?;
        publication.commit_ref(source.root)
    }

    pub fn move_ref(&self, expected: &RefState, target: ObjectId) -> EngineResult<RefState> {
        let mut publication = self.begin_publication(Some(expected), &expected.name)?;
        publication.retain_existing_root(target)?;
        publication.commit_ref(target)
    }

    pub fn retained_roots(&self) -> EngineResult<Vec<ObjectId>> {
        let connection = self.lock_connection()?;
        self.mark_statement()?;
        let mut statement = connection
            .prepare("SELECT root_id FROM layerfs_retained_roots ORDER BY root_id")
            .map_err(map_sqlite_error)?;
        let roots = statement
            .query_map([], |row| row.get::<_, Vec<u8>>(0))
            .map_err(map_sqlite_error)?
            .map(|row| {
                ObjectId::from_bytes(&row.map_err(map_sqlite_error)?).map_err(EngineError::Core)
            })
            .collect();
        roots
    }
}

pub(crate) fn read_ref_on_connection(
    connection: &Connection,
    name: &str,
) -> EngineResult<Option<RefState>> {
    connection
        .query_row(
            "SELECT generation, root_id FROM layerfs_refs WHERE name = ?1",
            params![name],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?)),
        )
        .optional()
        .map_err(map_sqlite_error)?
        .map(|(generation, root)| {
            Ok(RefState {
                name: name.to_owned(),
                generation: u64::try_from(generation)
                    .map_err(|_| EngineError::InvalidRecord("ref generation"))?,
                root: ObjectId::from_bytes(&root)?,
            })
        })
        .transpose()
}

pub(crate) fn validate_ref_name(name: &str) -> EngineResult<()> {
    if name.is_empty() || name.len() > 255 || name.bytes().any(|byte| byte == 0 || byte == b'/') {
        Err(EngineError::InvalidRecord("ref name"))
    } else {
        Ok(())
    }
}
