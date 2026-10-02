//! Strict catalog action dispatch; all request fields are validated before mutation.
use super::{Bytes, Encoder, BODY_PAGE};
use crate::strict_catalog::{PlacementDomain, StrictCatalog, PAGE};

pub(super) enum Error {
    Validation(String),
    Operation(String),
}
impl From<String> for Error {
    fn from(error: String) -> Self {
        Self::Validation(error)
    }
}
impl From<&str> for Error {
    fn from(error: &str) -> Self {
        Self::Validation(error.into())
    }
}
pub(super) fn run(catalog: &StrictCatalog, action: u8, payload: &[u8]) -> Result<Vec<u8>, Error> {
    let mut b = Bytes::new(payload);
    let mut out = Encoder::new();
    match action {
        0 => {
            let owner = b.optional()?;
            b.done()?;
            out.scope(catalog.capture(owner)?)?;
        }
        1 => {
            b.done()?;
            out.signed(catalog.begin_save().map_err(Error::Operation)?)?;
        }
        2 => {
            let id = b.id()?;
            b.done()?;
            let found = catalog.descriptor(id)?;
            out.byte(u8::from(found.is_some()))?;
            if let Some((role, length)) = found {
                out.byte(role.code())?;
                out.size(length)?;
            }
        }
        3 => {
            let scope = b.scope()?;
            let domain = b.domain()?;
            let count = b.count(PAGE)?;
            b.require(count, 32)?;
            let mut ids = Vec::with_capacity(count);
            for _ in 0..count {
                ids.push(b.id()?);
            }
            b.done()?;
            let rows = catalog.locations(scope, domain, &ids)?;
            out.count(rows.len(), PAGE)?;
            for row in rows {
                out.optional_located(row)?;
            }
        }
        4 => {
            let scope = b.scope()?;
            let id = b.id()?;
            let usage = b.usage()?;
            b.done()?;
            out.byte(u8::from(catalog.has_use(scope, id, usage)?))?;
        }
        5 => {
            let scope = b.scope()?;
            let domain = b.domain()?;
            let order = b.signed()?;
            let offset = b.size()?;
            let length = b.size()?;
            b.done()?;
            if length == 0 || length > BODY_PAGE {
                return Err("body page admission".into());
            }
            let (digest, total, bytes) = catalog.body_page(scope, domain, order, offset, length)?;
            out.raw(&digest)?;
            out.byte(u8::from(total.is_some()))?;
            if let Some(total) = total {
                out.size(total)?;
            }
            out.blob(&bytes)?;
        }
        6 => {
            let save = b.signed()?;
            let digest = b.take()?;
            b.done()?;
            out.signed(
                catalog
                    .register_body(PlacementDomain::FilePayload, save, digest, None)
                    .map_err(Error::Operation)?,
            )?;
        }
        7 => {
            let save = b.signed()?;
            let digest = b.take()?;
            let total = b.size()?;
            b.done()?;
            out.signed(
                catalog
                    .begin_metadata_body(save, digest, total)
                    .map_err(Error::Operation)?,
            )?;
        }
        8 => {
            let save = b.signed()?;
            let order = b.signed()?;
            let offset = b.size()?;
            let bytes = b.blob()?;
            b.done()?;
            catalog
                .append_metadata_body(save, order, offset, bytes)
                .map_err(Error::Operation)?;
        }
        9 => {
            let save = b.signed()?;
            let order = b.signed()?;
            b.done()?;
            catalog
                .finish_metadata_body(save, order)
                .map_err(Error::Operation)?;
        }
        10 => {
            let scope = b.scope()?;
            let save = b.signed()?;
            let domain = b.domain()?;
            let id = b.id()?;
            let usage = b.usage()?;
            let refs = b.refs()?;
            b.done()?;
            catalog
                .register_use(scope, save, domain, id, usage, &refs)
                .map_err(Error::Operation)?;
        }
        11 => {
            let scope = b.scope()?;
            let save = b.signed()?;
            let count = b.count(PAGE)?;
            let mut rows = Vec::with_capacity(count);
            for _ in 0..count {
                rows.push(b.registration()?);
            }
            b.done()?;
            let rows = catalog
                .register(scope, save, &rows)
                .map_err(Error::Operation)?;
            out.count(rows.len(), PAGE)?;
            for row in rows {
                out.located(row)?;
            }
        }
        12 => {
            let save = b.signed()?;
            let count = b.size()?;
            b.done()?;
            out.u32(
                catalog
                    .reserve_ordinals(save, count)
                    .map_err(Error::Operation)?,
            )?;
        }
        13 => {
            let scope = b.scope()?;
            let ordinal = b.u32()?;
            b.done()?;
            let row = catalog.group_for(scope, ordinal)?;
            out.byte(u8::from(row.is_some()))?;
            if let Some(row) = row {
                out.group(row)?;
            }
        }
        14 => {
            let scope = b.scope()?;
            let from = b.u32()?;
            let limit = b.count(PAGE)?;
            b.done()?;
            let rows = catalog.group_page(scope, from, limit)?;
            out.count(rows.len(), PAGE)?;
            for row in rows {
                out.group(row)?;
            }
        }
        15 => {
            let scope = b.scope()?;
            b.done()?;
            out.u32(catalog.metadata_window_start(scope)?)?;
        }
        16..=18 => {
            let save = b.signed()?;
            b.done()?;
            match action {
                16 => catalog.finish_storage(save).map_err(Error::Operation)?,
                17 => catalog.abandon(save).map_err(Error::Operation)?,
                _ => catalog.quarantine(save).map_err(Error::Operation)?,
            }
        }
        19 => {
            let scope = b.scope()?;
            let save = b.signed()?;
            let count = b.count(PAGE)?;
            b.require(count, 48)?;
            let mut rows = Vec::with_capacity(count);
            for _ in 0..count {
                rows.push(b.group()?);
            }
            b.done()?;
            catalog
                .insert_groups(scope, save, &rows)
                .map_err(Error::Operation)?;
        }
        20 => {
            let save = b.signed()?;
            let used = b.size()?;
            let first = b.u32()?;
            b.done()?;
            catalog
                .note_window(save, used, first)
                .map_err(Error::Operation)?;
        }
        21 => {
            let scope = b.scope()?;
            let order = b.signed()?;
            b.done()?;
            out.domain(catalog.body_domain(scope, order)?)?;
        }
        22 => {
            let domain = b.domain()?;
            let save = b.signed()?;
            let digest = b.take()?;
            b.done()?;
            out.signed(
                catalog
                    .reserve_body(domain, save, digest)
                    .map_err(Error::Operation)?,
            )?;
        }
        23 => {
            let save = b.signed()?;
            let order = b.signed()?;
            let digest = b.take()?;
            b.done()?;
            catalog
                .acknowledge_body_checked(save, order, digest, None)
                .map_err(Error::Operation)?;
        }
        24 => {
            let save = b.signed()?;
            let order = b.signed()?;
            b.done()?;
            catalog
                .discard_reserved_body(save, order)
                .map_err(Error::Operation)?;
        }
        25 => {
            let save = b.signed()?;
            let order = b.signed()?;
            let digest = b.take()?;
            let total = b.size()?;
            b.done()?;
            catalog
                .begin_reserved_metadata_body(save, order, digest, total)
                .map_err(Error::Operation)?;
        }
        26 => {
            let save = b.signed()?;
            let used = b.u64()?;
            let reserved = b.u64()?;
            b.done()?;
            catalog
                .release_ordinals(save, used, reserved)
                .map_err(Error::Operation)?;
        }
        27 => {
            let scope = b.scope()?;
            let domain = b.domain()?;
            let after = b.u64()?;
            let limit = b.count(PAGE)?;
            b.done()?;
            let rows = catalog.candidate_page(scope, domain, after, limit)?;
            out.count(rows.len(), PAGE)?;
            for row in rows {
                out.candidate(row)?;
            }
        }
        28 => {
            let scope = b.scope()?;
            let save = b.signed()?;
            let domain = b.domain()?;
            let n = b.count(PAGE)?;
            b.require(n, 74)?;
            let mut rows = Vec::with_capacity(n);
            for _ in 0..n {
                rows.push(b.candidate()?);
            }
            b.done()?;
            catalog
                .stage_candidates(scope, save, domain, &rows)
                .map_err(Error::Operation)?;
        }
        29 => {
            let context = b.save_context()?;
            b.done()?;
            out.signed(
                catalog
                    .begin_save_owned(&context)
                    .map_err(Error::Operation)?,
            )?;
        }
        _ => return Err("strict action unavailable".into()),
    }
    Ok(out.finish())
}
