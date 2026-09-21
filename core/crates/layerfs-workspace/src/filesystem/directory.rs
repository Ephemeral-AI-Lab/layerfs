use super::namespace::{attributes, child_path};
use crate::{
    runtime::state::{Cookie, COOKIE_LIMIT},
    *,
};
use layerfs_bridge::contract::{Inspect, Operation, Response};
use std::{mem::size_of, time::Instant};

impl Workspace {
    pub fn opendir(&self, serial: u64, scope: ReferenceScope) -> Result<HandleId, WorkspaceError> {
        self.open_handle(serial, true, scope)
    }
    pub fn readdir(
        &self,
        handle: HandleId,
        cookie: u64,
        limit: usize,
        deadline: Instant,
    ) -> Result<DirectoryPage, WorkspaceError> {
        if limit == 0 || limit > MAX_DIRECTORY_ENTRIES {
            return Err(WorkspaceError::InvalidInput);
        }
        let deadline = Self::callback_deadline(deadline);
        let operation = self.begin(true, deadline)?;
        let charge = self
            .host
            .budget
            .reserve(limit * (size_of::<DirectoryEntry>() + 255))?;
        let (path, serial, parent, dots, after, base) = {
            let state = self.state()?;
            let found = state.handle(handle, true)?;
            let node = state.node(found.serial)?;
            let (dots, after) = if cookie == 0 {
                (0, Vec::new())
            } else {
                let position = state
                    .cookies
                    .iter()
                    .find(|entry| entry.handle == handle && entry.id == cookie)
                    .ok_or(WorkspaceError::InvalidInput)?;
                (position.dots, position.after[..position.len].to_vec())
            };
            (
                node.path().to_vec(),
                node.attr.serial,
                node.parent,
                dots,
                after,
                state.base,
            )
        };
        let mut entries = Vec::new();
        entries
            .try_reserve_exact(limit)
            .map_err(|_| WorkspaceError::Capacity)?;
        if dots == 0 {
            entries.push(DirectoryEntry {
                serial,
                kind: NodeKind::Directory,
                name: b".".to_vec(),
                cookie: 0,
            });
        }
        if dots < 2 && entries.len() < limit {
            entries.push(DirectoryEntry {
                serial: parent,
                kind: NodeKind::Directory,
                name: b"..".to_vec(),
                cookie: 0,
            });
        }
        if entries.len() < limit {
            let response = self.call(
                Operation::Inspect {
                    root: base,
                    query: Inspect::List {
                        path: path.clone(),
                        after: after.clone(),
                        entries: (limit - entries.len()) as u16,
                        bytes: 16384,
                    },
                },
                0,
                &mut std::io::sink(),
                deadline,
            )?;
            let Response::List {
                entries: names,
                continuation,
            } = response
            else {
                return Err(WorkspaceError::InvalidInput);
            };
            if names.len() > limit - entries.len()
                || names.iter().map(|(name, _)| name.len() + 10).sum::<usize>() > 16384
                || continuation
                    .as_ref()
                    .is_some_and(|next| names.last().is_none_or(|(last, _)| next != last))
            {
                return Err(WorkspaceError::InvalidInput);
            }
            let mut previous = after;
            for (name, expected_serial) in names {
                if name <= previous {
                    return Err(WorkspaceError::InvalidInput);
                }
                let child = child_path(&path, &name)?;
                let response = self.call(
                    Operation::Inspect {
                        root: base,
                        query: Inspect::Attributes { path: child },
                    },
                    0,
                    &mut std::io::sink(),
                    deadline,
                )?;
                let (attr, _, _) =
                    attributes(response, false, self.inner.root.uid, self.inner.root.gid)?;
                if attr.serial != expected_serial {
                    return Err(WorkspaceError::InvalidInput);
                }
                previous = name.clone();
                entries.push(DirectoryEntry {
                    serial: attr.serial,
                    kind: attr.kind,
                    name,
                    cookie: 0,
                });
            }
        }
        {
            let mut state = self.state()?;
            state.handle(handle, true)?;
            let required = entries
                .iter()
                .filter(|entry| {
                    let (dots, name) = position(&entry.name);
                    !state.cookies.iter().any(|old| {
                        old.handle == handle && old.dots == dots && &old.after[..old.len] == name
                    })
                })
                .count();
            if state.cookies.len() + required > COOKIE_LIMIT {
                return Err(WorkspaceError::Capacity);
            }
            state
                .next_cookie
                .checked_add(required as u64)
                .ok_or(WorkspaceError::Capacity)?;
            for entry in &mut entries {
                let (dots, name) = position(&entry.name);
                if let Some(existing) = state.cookies.iter().find(|position| {
                    position.handle == handle
                        && position.dots == dots
                        && &position.after[..position.len] == name
                }) {
                    entry.cookie = existing.id;
                    continue;
                }
                let id = state.next_cookie;
                state.next_cookie += 1;
                let mut after = [0; 255];
                after[..name.len()].copy_from_slice(name);
                state.cookies.push(Cookie {
                    id,
                    handle,
                    after,
                    len: name.len(),
                    dots,
                });
                entry.cookie = id;
            }
        }
        Ok(DirectoryPage {
            entries,
            _charge: charge,
            _operation: operation,
        })
    }
    pub fn releasedir(&self, handle: HandleId) -> Result<(), WorkspaceError> {
        self.release_handle(handle, true)
    }
}
fn position(name: &[u8]) -> (u8, &[u8]) {
    match name {
        b"." => (1, &[][..]),
        b".." => (2, &[][..]),
        name => (2, name),
    }
}
