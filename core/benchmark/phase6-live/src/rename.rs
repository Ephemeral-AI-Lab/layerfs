//! Ordinary rename is one name transaction; descendants keep stable inode IDs.
use crate::engine::{now, Engine};
use rusqlite::params;
fn valid(name: &[u8]) -> bool {
    !name.is_empty()
        && name != b"."
        && name != b".."
        && name.len() <= 255
        && !name.contains(&0)
        && !name.contains(&b'/')
}
impl Engine {
    pub fn rename(
        &mut self,
        parent: i64,
        name: &[u8],
        new_parent: i64,
        new_name: &[u8],
        noreplace: bool,
    ) -> Result<(), String> {
        if !valid(name) || !valid(new_name) {
            return Err("EINVAL".into());
        }
        for id in [parent, new_parent] {
            let n = self.node(id)?;
            if n.kind != 2 {
                return Err("ENOTDIR".into());
            }
            if n.links == 0 {
                return Err("ENOENT".into());
            }
        }
        let id = self.lookup(parent, name)?.ok_or("ENOENT")?;
        let node = self.node(id)?;
        let target = self.lookup(new_parent, new_name)?;
        if noreplace && target.is_some() {
            return Err("EEXIST".into());
        }
        if target == Some(id) {
            return Ok(());
        }
        if node.kind == 2 {
            let mut ancestor = new_parent;
            let mut depth = 0;
            loop {
                if ancestor == id {
                    return Err("EINVAL".into());
                }
                if ancestor == 1 {
                    break;
                }
                depth += 1;
                if depth >= 512 {
                    return Err("EINVAL".into());
                }
                ancestor = self.node(ancestor)?.parent;
            }
        }
        let replaced = target.map(|id| self.node(id)).transpose()?;
        if let Some(n) = &replaced {
            if node.kind == 2 && n.kind != 2 {
                return Err("ENOTDIR".into());
            }
            if node.kind != 2 && n.kind == 2 {
                return Err("EISDIR".into());
            }
            if n.kind == 2 && n.children != 0 {
                return Err("ENOTEMPTY".into());
            }
        } else if parent != new_parent && self.node(new_parent)?.children >= 512 {
            return Err("ENOSPC".into());
        }
        let (sec, nano) = now();
        let tx = self.db.transaction().map_err(|e| e.to_string())?;
        tx.execute(
            "DELETE FROM names WHERE parent=?1 AND name=?2",
            params![parent, name],
        )
        .map_err(|e| e.to_string())?;
        tx.execute("INSERT INTO changed_names VALUES(?1,?2,NULL) ON CONFLICT(parent,name) DO UPDATE SET ino=NULL",params![parent,name]).map_err(|e|e.to_string())?;
        if let Some(n) = &replaced {
            tx.execute(
                "UPDATE inodes SET links=links-1,dirty=1 WHERE id=?1",
                [n.id],
            )
            .map_err(|e| e.to_string())?;
        }
        tx.execute("INSERT INTO names VALUES(?1,?2,?3) ON CONFLICT(parent,name) DO UPDATE SET ino=excluded.ino",params![new_parent,new_name,id]).map_err(|e|e.to_string())?;
        tx.execute("INSERT INTO changed_names VALUES(?1,?2,?3) ON CONFLICT(parent,name) DO UPDATE SET ino=excluded.ino",params![new_parent,new_name,id]).map_err(|e|e.to_string())?;
        tx.execute("UPDATE inodes SET children=children-1,subdirs=subdirs-?4,dirty=1,seconds=?2,nanos=?3 WHERE id=?1",params![parent,sec,nano,i64::from(node.kind==2)]).map_err(|e|e.to_string())?;
        tx.execute("UPDATE inodes SET children=children+?4,subdirs=subdirs+?5,dirty=1,seconds=?2,nanos=?3 WHERE id=?1",params![new_parent,sec,nano,i64::from(replaced.is_none()),i64::from(node.kind==2)-i64::from(replaced.as_ref().is_some_and(|n|n.kind==2))]).map_err(|e|e.to_string())?;
        tx.execute(
            "UPDATE inodes SET parent=?2 WHERE id=?1",
            params![id, new_parent],
        )
        .map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        self.revision += 1;
        Ok(())
    }
}
