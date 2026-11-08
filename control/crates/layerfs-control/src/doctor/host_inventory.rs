//! Metadata of the management paths Host actually observes, never runtime trees.
use super::Budget;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    path::Path,
};

#[derive(Default)]
struct Inventory {
    count: u64,
    xor: u64,
    sum: u64,
}
impl Inventory {
    fn path(&mut self, path: &Path, budget: &Budget) -> Result<bool, ()> {
        if budget.expired() {
            return Err(());
        }
        let mut hash = DefaultHasher::new();
        path.hash(&mut hash);
        let exists = match std::fs::symlink_metadata(path) {
            Ok(meta) => {
                // Directory membership is fingerprinted by selected children,
                // not volatile mtime/size from SQLite/log/runtime bookkeeping.
                (meta.ino(), meta.dev(), meta.mode()).hash(&mut hash);
                if meta.is_symlink() {
                    return Err(());
                }
                if !meta.is_dir() {
                    (meta.len(), meta.mtime(), meta.mtime_nsec()).hash(&mut hash);
                }
                true
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                false.hash(&mut hash);
                false
            }
            Err(_) => return Err(()),
        };
        let value = hash.finish();
        self.count += 1;
        self.xor ^= value;
        self.sum = self.sum.wrapping_add(value);
        Ok(exists)
    }
    fn entries(
        &mut self,
        path: &Path,
        budget: &Budget,
        mut visit: impl FnMut(&mut Self, &Path, bool) -> Result<(), ()>,
    ) -> Result<(), ()> {
        if !self.path(path, budget)? {
            return Ok(());
        }
        for entry in std::fs::read_dir(path).map_err(|_| ())? {
            if budget.expired() {
                return Err(());
            }
            let entry = entry.map_err(|_| ())?;
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            let directory = entry.file_type().map_err(|_| ())?.is_dir();
            visit(self, &entry.path(), directory)?;
        }
        Ok(())
    }
    fn json_files(&mut self, path: &Path, budget: &Budget) -> Result<(), ()> {
        self.entries(path, budget, |inventory, path, directory| {
            if !directory && path.extension().is_some_and(|x| x == "json") {
                inventory.path(path, budget)?;
            }
            Ok(())
        })
    }
}

pub(super) fn fingerprint(home: &Path, budget: &Budget) -> Result<u64, ()> {
    let mut inventory = Inventory::default();
    inventory.path(home, budget)?;
    inventory.path(&home.join("layerfs.json"), budget)?;
    for kind in ["machine", "store", "sandbox", "backup"] {
        inventory.entries(&home.join(kind), budget, |inventory, entity, directory| {
            if !directory {
                return Ok(());
            }
            inventory.path(entity, budget)?;
            let files: &[&str] = match kind {
                "machine" => &["record.json", "worker.json"],
                "sandbox" => &["record.json", "control.json"],
                _ => &["record.json"],
            };
            for file in files {
                inventory.path(&entity.join(file), budget)?;
            }
            match kind {
                "store" => {
                    inventory.path(&entity.join("store.sqlite.incoming"), budget)?;
                    inventory.entries(
                        &entity.join("open"),
                        budget,
                        |inventory, path, directory| {
                            if !directory
                                && path
                                    .file_name()
                                    .and_then(|n| n.to_str())
                                    .is_some_and(|n| n.parse::<u32>().is_ok())
                            {
                                inventory.path(path, budget)?;
                            }
                            Ok(())
                        },
                    )?;
                    inventory.entries(
                        &entity.join("project"),
                        budget,
                        |inventory, path, directory| {
                            if directory {
                                inventory.json_files(path, budget)?;
                            }
                            Ok(())
                        },
                    )?;
                }
                "sandbox" => inventory.entries(
                    &entity.join("workspace"),
                    budget,
                    |inventory, path, directory| {
                        if directory {
                            inventory.path(path, budget)?;
                            inventory.json_files(&path.join("exec"), budget)?;
                        } else if path.extension().is_some_and(|x| x == "json") {
                            inventory.path(path, budget)?;
                        }
                        Ok(())
                    },
                )?,
                _ => {}
            }
            Ok(())
        })?;
    }
    // Order-independent accumulation avoids collecting/sorting runtime- or
    // directory-sized resident vectors just to compare metadata observations.
    let mut hash = DefaultHasher::new();
    (inventory.count, inventory.xor, inventory.sum).hash(&mut hash);
    Ok(hash.finish())
}
