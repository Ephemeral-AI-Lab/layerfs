//! Paired selected-root cursors; skip identical subtrees before expanding rows.
use crate::tree_facts::{self, Item, Node};
use layerfs_content::ObjectId;
use rusqlite::Connection;
#[derive(Default, Debug)]
pub struct Work {
    pub expanded: u64,
    pub compared: u64,
    pub skipped: u64,
    pub changed: u64,
    pub peak_items: usize,
    pub peak_owned_bytes: usize,
}
#[derive(Clone)]
enum Front {
    Node(ObjectId, Node),
    Row(Item),
}
struct Cursor {
    items: Vec<Front>,
}
impl Cursor {
    fn new(db: &Connection, root: Option<ObjectId>) -> Result<Self, String> {
        Ok(Self {
            items: root
                .map(|id| tree_facts::node(db, id).map(|n| vec![Front::Node(id, n)]))
                .transpose()?
                .unwrap_or_default(),
        })
    }
    fn bytes(&self) -> usize {
        self.items.capacity() * std::mem::size_of::<Front>()
            + self
                .items
                .iter()
                .map(|f| match f {
                    Front::Node(_, n) => n.min.capacity() + n.max.capacity(),
                    Front::Row(r) => r.key.capacity() + r.value.as_ref().map_or(0, Vec::capacity),
                })
                .sum::<usize>()
    }
    fn expand(
        &mut self,
        db: &Connection,
        other_bytes: usize,
        work: &mut Work,
    ) -> Result<(), String> {
        let Some(Front::Node(id, node)) = self.items.pop() else {
            return Err("diff cursor node".into());
        };
        let bound = self.bytes()
            + other_bytes
            + node.entries * (std::mem::size_of::<Front>() + 1024)
            + 32768;
        if bound > 4 * 1024 * 1024 {
            return Err("diff shared cursor byte admission".into());
        }
        self.items
            .try_reserve_exact(node.entries)
            .map_err(|_| "diff cursor reservation")?;
        work.expanded += 1;
        for j in (0..node.entries).rev() {
            let row = tree_facts::item(db, id, j)?;
            self.items.push(if let Some(id) = row.child {
                Front::Node(id, tree_facts::node(db, id)?)
            } else {
                Front::Row(row)
            });
        }
        // Each path retains bounded sibling pages, never a population frontier.
        if self.items.len() > 32 * layerfs_content::filesystem::limits::MAXIMUM_DIRECTORY_LEAF_ROWS
        {
            return Err("diff cursor width/depth admission".into());
        }
        work.peak_items = work.peak_items.max(self.items.len());
        Ok(())
    }
}
pub fn run(
    db: &Connection,
    old: Option<ObjectId>,
    new: Option<ObjectId>,
    mut change: impl FnMut(&[u8], Option<&[u8]>, Option<&[u8]>) -> Result<(), String>,
) -> Result<Work, String> {
    let mut a = Cursor::new(db, old)?;
    let mut b = Cursor::new(db, new)?;
    let mut w = Work::default();
    loop {
        w.peak_items = w.peak_items.max(a.items.len() + b.items.len());
        w.peak_owned_bytes = w.peak_owned_bytes.max(a.bytes() + b.bytes());
        if a.bytes() + b.bytes() > 4 * 1024 * 1024 {
            return Err("diff shared owned-byte admission".into());
        }
        match (a.items.last().cloned(), b.items.last().cloned()) {
            (None, None) => break,
            (Some(Front::Node(x, _)), Some(Front::Node(y, _))) if x == y => {
                a.items.pop();
                b.items.pop();
                w.skipped += 1
            }
            (Some(Front::Node(_, x)), Some(Front::Node(_, y))) => {
                if x.max < y.min || x.level > y.level {
                    a.expand(db, b.bytes(), &mut w)?
                } else if y.max < x.min || y.level > x.level {
                    b.expand(db, a.bytes(), &mut w)?
                } else {
                    a.expand(db, b.bytes(), &mut w)?;
                    b.expand(db, a.bytes(), &mut w)?
                }
            }
            (Some(Front::Row(x)), Some(Front::Row(y))) => {
                w.compared += 1;
                match x.key.cmp(&y.key) {
                    std::cmp::Ordering::Less => {
                        change(&x.key, x.value.as_deref(), None)?;
                        a.items.pop();
                        w.changed += 1
                    }
                    std::cmp::Ordering::Greater => {
                        change(&y.key, None, y.value.as_deref())?;
                        b.items.pop();
                        w.changed += 1
                    }
                    std::cmp::Ordering::Equal => {
                        if x.value != y.value {
                            change(&x.key, x.value.as_deref(), y.value.as_deref())?;
                            w.changed += 1
                        }
                        a.items.pop();
                        b.items.pop();
                    }
                }
            }
            (Some(Front::Row(x)), Some(Front::Node(_, y))) if x.key < y.min => {
                change(&x.key, x.value.as_deref(), None)?;
                a.items.pop();
                w.changed += 1
            }
            (Some(Front::Node(_, x)), Some(Front::Row(y))) if y.key < x.min => {
                change(&y.key, None, y.value.as_deref())?;
                b.items.pop();
                w.changed += 1
            }
            (Some(Front::Node(..)), _) => a.expand(db, b.bytes(), &mut w)?,
            (_, Some(Front::Node(..))) => b.expand(db, a.bytes(), &mut w)?,
            (Some(Front::Row(x)), None) => {
                change(&x.key, x.value.as_deref(), None)?;
                a.items.pop();
                w.changed += 1
            }
            (None, Some(Front::Row(y))) => {
                change(&y.key, None, y.value.as_deref())?;
                b.items.pop();
                w.changed += 1
            }
        }
    }
    Ok(w)
}
