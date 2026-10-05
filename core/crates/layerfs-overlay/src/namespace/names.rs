//! Bounded current namespace inputs over an exact unchanged base source.
use crate::{
    db::{integer, unsigned},
    sql, BaseSource, Dentry, Generation, Overlay, OverlayError, OverlayResult, StatementKind,
};
fn decode(row: &rusqlite::Row<'_>) -> rusqlite::Result<Dentry> {
    Ok(Dentry {
        inherited: row.get(3)?,
        parent: unsigned(row, 0)?,
        name: row.get(1)?,
        serial: row
            .get::<_, Option<i64>>(2)?
            .map(|v| u64::try_from(v).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(2, v)))
            .transpose()?,
    })
}
impl Overlay {
    pub fn source_name_window(
        &self,
        source: BaseSource,
        parent: u64,
        after: Option<&[u8]>,
    ) -> OverlayResult<crate::NameWindow> {
        let state = self.source_state(source)?;
        let parent_inode = self.inode_at(source.route, parent, state.active, state.installed)?;
        let active = self.source_names(source, parent, state.active, after)?;
        let captured = if let Some(generation) = state.captured.or(state.consolidating) {
            self.source_names(source, parent, generation, after)?
        } else {
            Vec::new()
        };
        Ok(crate::NameWindow {
            source,
            parent,
            parent_inode,
            active,
            captured,
        })
    }
    pub fn source_generations(
        &self,
        source: BaseSource,
    ) -> OverlayResult<(Generation, Option<Generation>)> {
        let state = self.source_state(source)?;
        Ok((state.active, state.captured.or(state.consolidating)))
    }
    pub fn source_dentry(
        &self,
        source: BaseSource,
        parent: u64,
        name: &[u8],
    ) -> OverlayResult<Option<Dentry>> {
        if name.is_empty() || name.len() > 255 {
            return Err(OverlayError::Invalid("source name"));
        }
        let state = self.source_state(source)?;
        self.query(
            StatementKind::Dentry,
            sql::DENTRY_LOOKUP,
            &[
                &source.route.ns,
                &integer(parent)?,
                &name,
                &state.active.0,
                &state.installed,
            ],
            32 + name.len() as u64,
            |row| {
                Ok(Dentry {
                    inherited: row.get(1)?,
                    parent,
                    name: name.to_vec(),
                    serial: row
                        .get::<_, Option<i64>>(0)?
                        .map(|v| {
                            u64::try_from(v)
                                .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(0, v))
                        })
                        .transpose()?,
                })
            },
        )
        .map(|mut rows| rows.pop())
    }
    pub fn source_names(
        &self,
        source: BaseSource,
        parent: u64,
        generation: Generation,
        after: Option<&[u8]>,
    ) -> OverlayResult<Vec<Dentry>> {
        let state = self.source_state(source)?;
        if generation != state.active && Some(generation) != state.captured.or(state.consolidating)
        {
            return Err(OverlayError::Stale);
        }
        let after = after.unwrap_or(&[]);
        if after.len() > 255 {
            return Err(OverlayError::Invalid("source name cursor"));
        }
        self.query(
            StatementKind::Dentry,
            sql::SOURCE_NAMES,
            &[&source.route.ns, &generation.0, &integer(parent)?, &after],
            24 + after.len() as u64,
            decode,
        )
    }
    pub fn explain_source_names(
        &self,
        source: BaseSource,
        parent: u64,
        generation: Generation,
        after: Option<&[u8]>,
    ) -> OverlayResult<Vec<String>> {
        self.source_state(source)?;
        let after = after.unwrap_or(&[]);
        if after.len() > 255 {
            return Err(OverlayError::Invalid("source name cursor"));
        }
        self.query(
            StatementKind::Explain,
            &format!("EXPLAIN QUERY PLAN {}", sql::SOURCE_NAMES),
            &[&source.route.ns, &generation.0, &integer(parent)?, &after],
            24 + after.len() as u64,
            |row| row.get(3),
        )
    }
}
