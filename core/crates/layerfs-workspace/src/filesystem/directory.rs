use crate::{
    runtime::state::{Cookie, COOKIE_LIMIT},
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
        let (path, serial, parent, dots, after, view) = {
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
                (serial, &path),
                &after,
                limit - entries.len(),
                deadline,
            )?;
            for (name, expected_serial) in names {
                let resolved =
                    self.resolve_child(&mut operation, &view, serial, &path, &name, deadline)?;
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
