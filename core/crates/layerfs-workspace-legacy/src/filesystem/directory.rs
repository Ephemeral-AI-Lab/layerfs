use crate::{
    runtime::state::{cookie_key, Cookie, COOKIE_ENTRY_BYTES},
    *,
};
use std::{mem::size_of, time::Instant};

impl Workspace {
    pub fn opendir(&self, serial: u64, scope: ReferenceScope) -> Result<HandleId, WorkspaceError> {
        self.open_directory_handle(serial, scope)
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
        let mut operation = self.begin(false, deadline)?;
        operation.local_io()?;
        let charge = self
            .host
            .budget
            .reserve(limit * (size_of::<DirectoryEntry>() + 255))?;
        let (serial, parent, dots, after, view) = {
            let state = self.state()?;
            let found = state.handle(handle, true)?;
            let node = state.node(found.serial)?;
            let (dots, after) = if cookie == 0 {
                (0, Vec::new())
            } else {
                let position = state
                    .cookies
                    .get(&cookie)
                    .filter(|entry| entry.handle == handle)
                    .ok_or(WorkspaceError::InvalidInput)?;
                (position.dots, position.after[..position.len].to_vec())
            };
            (
                node.attr.serial,
                node.parent,
                dots,
                after,
                found.view.ok_or(WorkspaceError::Io)?,
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
            let names = self.list_view(
                &mut operation,
                &view,
                serial,
                &after,
                limit - entries.len(),
                deadline,
            )?;
            for (name, expected_serial) in names {
                let resolved =
                    self.resolve_child(&mut operation, &view, serial, &name, deadline)?;
                if resolved.attr.serial != expected_serial {
                    return Err(WorkspaceError::InvalidInput);
                }
                entries.push(DirectoryEntry {
                    serial: expected_serial,
                    kind: resolved.attr.kind,
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
                    !state
                        .cookie_names
                        .contains_key(&cookie_key(handle, dots, name))
                })
                .count();
            let count = state
                .cookies
                .len()
                .checked_add(required)
                .ok_or(WorkspaceError::Capacity)?;
            state.cookie_charge.resize(
                count
                    .checked_mul(COOKIE_ENTRY_BYTES)
                    .ok_or(WorkspaceError::Capacity)?,
            )?;
            state
                .next_cookie
                .checked_add(required as u64)
                .ok_or(WorkspaceError::Capacity)?;
            for entry in &mut entries {
                let (dots, name) = position(&entry.name);
                let key = cookie_key(handle, dots, name);
                if let Some(&existing) = state.cookie_names.get(&key) {
                    entry.cookie = existing;
                    continue;
                }
                let id = state.next_cookie;
                state.next_cookie += 1;
                state.cookies.insert(
                    id,
                    Cookie {
                        handle,
                        after: key.2,
                        len: name.len(),
                        dots,
                    },
                );
                state.cookie_names.insert(key, id);
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
