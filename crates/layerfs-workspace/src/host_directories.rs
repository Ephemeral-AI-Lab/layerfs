//! Stable directory cookies indexed with current bindings, never a daemon-sized
//! directory map or a retained history of removed names.
use crate::host_overlay::HostOverlay;
use crate::overlay::Mutation;
use layerfs_content::CanonicalName;
use layerfs_layerstack_store::{Result, StoreError};
use layerfs_workspace_core::{Attr, NodeId};
use std::ops::Bound;

const COOKIE_NAME: u8 = 8;
const COOKIE_ORDER: u8 = 9;
const COOKIE_STATE: u8 = 10;
const PAGE: usize = 128;
type Page = Vec<(u64, Attr, Vec<u8>)>;

fn prefix(domain: u8, parent: NodeId) -> Vec<u8> {
    [vec![domain], parent.0.to_be_bytes().to_vec()].concat()
}
fn number(bytes: &[u8]) -> Result<u64> {
    Ok(u64::from_be_bytes(bytes.try_into().map_err(|_| {
        StoreError::Integrity("directory cookie integer")
    })?))
}
struct State {
    next: u64,
    complete: bool,
    after: Option<Vec<u8>>,
}
impl State {
    fn decode(bytes: Option<Vec<u8>>) -> Result<Self> {
        let Some(bytes) = bytes else {
            return Ok(Self {
                next: 3,
                complete: false,
                after: None,
            });
        };
        if bytes.len() < 9 || bytes[8] > 1 {
            return Err(StoreError::Integrity("directory cookie state"));
        }
        let next = number(&bytes[..8])?;
        if !(3..=i64::MAX as u64).contains(&next) {
            return Err(StoreError::Integrity("directory cookie highwater"));
        }
        let after = if bytes.len() == 9 {
            None
        } else {
            CanonicalName::from_bytes(&bytes[9..])?;
            Some(bytes[9..].to_vec())
        };
        Ok(Self {
            next,
            complete: bytes[8] == 1,
            after,
        })
    }
    fn encode(&self) -> Vec<u8> {
        let mut bytes = self.next.to_be_bytes().to_vec();
        bytes.push(self.complete.into());
        bytes.extend_from_slice(self.after.as_deref().unwrap_or_default());
        bytes
    }
}

impl Mutation<'_> {
    pub(super) fn store_cookie_binding(
        &mut self,
        parent: NodeId,
        name: &[u8],
        node: Option<NodeId>,
    ) -> Result<()> {
        let name_key = [prefix(COOKIE_NAME, parent), name.to_vec()].concat();
        if let Some(bytes) = self.index.get(&self.candidate.index, &name_key)? {
            let cookie = number(&bytes)?;
            let order_key = [prefix(COOKIE_ORDER, parent), cookie.to_be_bytes().to_vec()].concat();
            let old = self
                .index
                .get(&self.candidate.index, &order_key)?
                .ok_or(StoreError::Integrity("directory cookie inverse missing"))?;
            if old.len() < 9 || old[8..] != *name {
                return Err(StoreError::Integrity("directory cookie inverse"));
            }
            if node == Some(NodeId(number(&old[..8])?)) {
                return Ok(());
            }
            self.candidate.index = self.index.remove(&self.candidate.index, &name_key)?;
            self.candidate.index = self.index.remove(&self.candidate.index, &order_key)?;
        }
        if let Some(node) = node {
            let state_key = prefix(COOKIE_STATE, parent);
            let mut state = State::decode(self.index.get(&self.candidate.index, &state_key)?)?;
            let cookie = state.next;
            state.next = cookie
                .checked_add(1)
                .filter(|n| *n <= i64::MAX as u64)
                .ok_or(StoreError::InvalidInput("directory cookie exhausted"))?;
            let order_key = [prefix(COOKIE_ORDER, parent), cookie.to_be_bytes().to_vec()].concat();
            let value = [node.0.to_be_bytes().to_vec(), name.to_vec()].concat();
            self.candidate.index =
                self.index
                    .set(&self.candidate.index, &state_key, &state.encode())?;
            self.candidate.index =
                self.index
                    .set(&self.candidate.index, &name_key, &cookie.to_be_bytes())?;
            self.candidate.index = self.index.set(&self.candidate.index, &order_key, &value)?;
        }
        Ok(())
    }
    pub(super) fn remove_cookie_directory(&mut self, node: NodeId) -> Result<()> {
        self.candidate.index = self
            .index
            .remove(&self.candidate.index, &prefix(COOKIE_STATE, node))?;
        Ok(())
    }
    fn cookie_rows(
        &self,
        parent: NodeId,
        after: u64,
        limit: usize,
    ) -> Result<Vec<(u64, NodeId, Vec<u8>)>> {
        let start = [prefix(COOKIE_ORDER, parent), after.to_be_bytes().to_vec()].concat();
        let stop = if parent.0 == u64::MAX {
            vec![COOKIE_ORDER + 1]
        } else {
            prefix(COOKIE_ORDER, NodeId(parent.0 + 1))
        };
        self.index
            .scan(
                &self.candidate.index,
                Bound::Excluded(&start),
                Bound::Excluded(&stop),
                limit,
            )?
            .into_iter()
            .map(|(key, value)| {
                if key.len() != 17 || value.len() < 9 {
                    return Err(StoreError::Integrity("directory cookie row"));
                }
                CanonicalName::from_bytes(&value[8..])?;
                Ok((
                    number(&key[9..])?,
                    NodeId(number(&value[..8])?),
                    value[8..].to_vec(),
                ))
            })
            .collect()
    }
}

impl HostOverlay {
    pub(crate) fn directory_cookies(&self, node: NodeId, after: u64, kernel: bool) -> Result<Page> {
        if after > i64::MAX as u64 {
            return Err(StoreError::InvalidInput("directory offset"));
        }
        loop {
            // Each scan/import/selection installs one bounded candidate. Other
            // mutations and snapshot acquisition can run between masked pages.
            let (page, done) = self.mutate(|m| {
                let directory = self.directory(&m.view(), node)?;
                let state_key = prefix(COOKIE_STATE, node);
                let mut state = State::decode(m.index.get(&m.candidate.index, &state_key)?)?;
                if after >= state.next {
                    return Err(StoreError::InvalidInput("directory offset not issued"));
                }
                let mut rows = m.cookie_rows(node, after.max(2), PAGE)?;
                if rows.len() < PAGE && !state.complete {
                    let imported = self.directory_page_in(m, node, state.after.as_deref())?;
                    // Import may have allocated cookies and advanced next; retain
                    // its highwater when recording only the base scan position.
                    state = State::decode(m.index.get(&m.candidate.index, &state_key)?)?;
                    state.complete = imported.continuation.is_none();
                    state.after = imported.continuation;
                    m.candidate.index =
                        m.index
                            .set(&m.candidate.index, &state_key, &state.encode())?;
                    rows = m.cookie_rows(node, after.max(2), PAGE)?;
                }
                let mut output = Vec::with_capacity(PAGE);
                if after == 0 {
                    output.push((1, directory.attr, b".".to_vec()));
                }
                if after < 2 {
                    let parent = directory
                        .parent
                        .ok_or(StoreError::Integrity("directory parent"))?;
                    output.push((2, self.metadata(&m.view(), parent)?.attr, b"..".to_vec()));
                }
                for (cookie, child, name) in rows.into_iter().take(PAGE - output.len()) {
                    let attr = self.metadata(&m.view(), child)?.attr;
                    if kernel {
                        self.retain_kernel_in(m, child)?;
                    }
                    output.push((cookie, attr, name));
                }
                Ok((output, state.complete))
            })?;
            if !page.is_empty() || done {
                return Ok(page);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host_overlay::tests::Fixture;
    use layerfs_workspace_core::{ResourcePolicy, ROOT};
    use std::collections::BTreeSet;
    use std::fs;

    #[test]
    fn cold_directory_cookies_survive_partial_pages_removal_and_bounded_churn() {
        let fixture = Fixture::new(
            |source| {
                for n in 0..300 {
                    fs::write(source.join(format!("n{n:03}")), b"x").unwrap();
                }
                fs::hard_link(source.join("n200"), source.join("alias")).unwrap();
            },
            ResourcePolicy::default(),
        );
        let host = &fixture.host;
        let before_invalid = host.snapshot().unwrap();
        let index_before = host.index.stats().unwrap();
        assert!(host
            .directory_cookies(ROOT, i64::MAX as u64, false)
            .is_err());
        assert_eq!(
            host.index.stats().unwrap().page_writes,
            index_before.page_writes
        );
        assert_eq!(
            host.snapshot().unwrap().root.installation_sequence,
            before_invalid.root.installation_sequence
        );
        // Acquiring a late lexical name first must not cause lazy enumeration
        // to skip the earlier canonical names or merge hardlink cookies.
        let alias = host.lookup(ROOT, b"n200").unwrap().node;
        let mut names = BTreeSet::new();
        let mut after = 0;
        let mut pages = 0;
        loop {
            let page = host.directory_cookies(ROOT, after, false).unwrap();
            assert!(page.len() <= PAGE);
            if page.is_empty() {
                break;
            }
            pages += 1;
            for (cookie, attr, name) in page {
                assert!(cookie > after);
                after = cookie;
                assert!(
                    names.insert(name.clone()),
                    "duplicate name in stable directory"
                );
                if name == b"alias" {
                    assert_eq!(attr.node, alias);
                }
            }
        }
        assert_eq!(names.len(), 303);
        for n in 0..300 {
            assert!(names.contains(format!("n{n:03}").as_bytes()));
        }
        assert!(pages >= 3);
        assert_eq!(
            host.snapshot().unwrap().root.sequence,
            0,
            "enumeration is not a logical mutation"
        );
        let initial = host.directory_cookies(ROOT, 0, true).unwrap();
        let (removed_cookie, removed, removed_name) = initial
            .iter()
            .find(|(_, attr, name)| {
                name.as_slice() != b"." && name.as_slice() != b".." && attr.node != alias
            })
            .unwrap()
            .clone();
        host.unlink(ROOT, &removed_name, false).unwrap();
        assert_eq!(
            host.attr(removed.node).unwrap().links,
            0,
            "kernel reference owns removed inode"
        );
        let resumed = host.directory_cookies(ROOT, removed_cookie, false).unwrap();
        assert!(resumed
            .iter()
            .all(|(cookie, _, name)| *cookie > removed_cookie && *name != removed_name));
        for (cookie, attr, _) in initial {
            if cookie >= 3 {
                host.kernel_forget(attr.node, 1).unwrap();
            }
        }
        assert!(host.attr(removed.node).is_err());

        // Removed names have no surviving cookie rows. Monotonic numbers do not
        // imply retaining one row for every operation since the directory opened.
        for _ in 0..64 {
            host.create_file(ROOT, b"temporary", 0o600).unwrap();
            host.unlink(ROOT, b"temporary", false).unwrap();
        }
        let remaining = host
            .mutate(|m| {
                let mut count = 0;
                let mut after = 2;
                loop {
                    let rows = m.cookie_rows(ROOT, after, PAGE)?;
                    if rows.is_empty() {
                        break;
                    }
                    after = rows.last().unwrap().0;
                    count += rows.len();
                }
                Ok(count)
            })
            .unwrap();
        assert_eq!(remaining, 300);
        let fresh = host.create_file(ROOT, b"after-eof", 0o600).unwrap();
        let next = host.directory_cookies(ROOT, after, false).unwrap();
        assert_eq!(next.len(), 1);
        assert_eq!(next[0].1.node, fresh.node);
        println!("301 cold names + dots across {pages} bounded pages; independent alias cookie; partial-reply FORGET; removed/churn rows reclaimed: PASS");
    }
}
