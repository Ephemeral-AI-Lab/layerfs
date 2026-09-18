//! The filesystem fixture builder: a recipe in, a checked input out.
//!
//! `FilesystemInput::check` refuses anything that is not strictly ordered —
//! `DirectoryUpdate.changes` by name, `directories` by parent, `inodes` by serial,
//! `new_inodes` sorted and unique — so a recipe cannot be turned into an input by
//! hand without those rules being applied somewhere. This module is that somewhere,
//! and it is the only place they are applied.
//!
//! **Why a serializable artifact.** The recipe is a pure function of
//! `(profile, entries, directories, seed)`, so the input could be rebuilt per
//! sample. `--emit-input`/`--load-input` exist anyway because a prepared input is
//! an artifact with an identity: the runner can cache it once, and a later sample
//! loads the same bytes rather than rebuilding a fixture it then has to trust. The
//! derived views (the per-directory listing oracle, the file list) are recomputed
//! from the loaded input by the same code path the builder uses, so a loaded
//! artifact cannot disagree with a built one about what it contains.
//!
//! **The serial layout, fixed here so every family agrees.** The root directory is
//! serial 1. Derived directories take `2..=1+directories`, in path order. Regular
//! files follow, in path order. A caller that needs a different layout does not
//! exist yet, and inventing one per family would make two families' fixtures
//! incomparable.

use std::collections::BTreeMap;
use std::path::Path;

use layerfs_content::filesystem::{DirectoryUpdate, InodeUpdate, PathName};
use layerfs_content::object::inode_leaf::{InodeKind, InodeValue};
use layerfs_content::ObjectId;

/// Root directory serial of every recipe.
pub const ROOT_SERIAL: u64 = 1;

/// Files a derived directory holds before another is added.
pub const FILES_PER_DIRECTORY: u32 = 8;

/// Largest number of derived directories.
pub const MAXIMUM_DIRECTORIES: u32 = 16;

/// First line of a serialized prepared tree.
pub const FORMAT: &str = "fs-bench-prepared-tree-v1";

/// One fixture recipe.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Recipe {
    /// Fixture profile as the registry names it.
    pub profile: &'static str,
    /// Regular files the tree holds.
    pub entries: u32,
    /// Directories to derive; `0` derives them from `entries`.
    pub directories: u32,
    /// Declared seed, so a fixture is a function of the row and never of the run.
    pub seed: u64,
}

impl Recipe {
    /// The directory count this recipe uses.
    pub fn directory_count(&self) -> u32 {
        if self.directories > 0 {
            return self.directories.min(self.entries.max(1));
        }
        let needed = self.entries.div_ceil(FILES_PER_DIRECTORY).max(1);
        needed.min(MAXIMUM_DIRECTORIES)
    }

    /// Files the `index`-th directory holds, so the blocks tile `entries` exactly.
    pub fn files_in(&self, index: u32) -> u32 {
        let directories = self.directory_count();
        let base = self.entries / directories;
        let remainder = self.entries % directories;
        base + u32::from(index < remainder)
    }

    /// The serial of the `index`-th directory.
    pub fn directory_serial(&self, index: u32) -> u64 {
        2 + u64::from(index)
    }

    /// The serial of the `index`-th file, in path order.
    pub fn file_serial(&self, index: u32) -> u64 {
        2 + u64::from(self.directory_count()) + u64::from(index)
    }
}

/// One prepared input, with the derived views the O4 oracle compares against.
pub struct PreparedTree {
    /// The recipe it was built from.
    pub recipe: Recipe,
    /// Final bindings, sorted by parent.
    pub directories: Vec<DirectoryUpdate>,
    /// Typed final values, sorted by serial.
    pub inodes: Vec<InodeUpdate>,
    /// Declared new serials, sorted and unique.
    pub new_inodes: Vec<u64>,
    /// One entry per directory: canonical path and its sorted bindings.
    pub listings: Vec<(String, Vec<(String, u64)>)>,
    /// Every regular file: canonical path and serial, sorted by path.
    pub files: Vec<(String, u64)>,
    /// Every directory serial, sorted.
    pub directory_serials: Vec<u64>,
}

/// The content root of one fixture node. Deterministic in the recipe.
fn content_root(profile: &str, seed: u64, label: &str) -> ObjectId {
    ObjectId::for_bytes(format!("layerfs/fs-bench/{profile}/{seed}/{label}").as_bytes())
}

fn metadata_root(profile: &str, seed: u64, label: &str) -> ObjectId {
    ObjectId::for_bytes(format!("layerfs/fs-bench/{profile}/{seed}/{label}/metadata").as_bytes())
}

/// A directory name for the `index`-th derived directory.
pub fn directory_name(index: u32) -> String {
    format!("d{index:04}")
}

/// A file name for the `index`-th file inside its directory.
pub fn file_name(index: u32) -> String {
    format!("f{index:06}")
}

fn name(text: &str) -> PathName {
    PathName::new(text).expect("a generated name is canonical")
}

fn directory_value(recipe: &Recipe, label: &str) -> InodeValue {
    InodeValue {
        kind: InodeKind::Directory,
        namespace_ref_count: 0,
        content_root: content_root(recipe.profile, recipe.seed, label),
        metadata_root: metadata_root(recipe.profile, recipe.seed, label),
    }
}

fn file_value(recipe: &Recipe, label: &str) -> InodeValue {
    InodeValue {
        kind: InodeKind::RegularFile,
        namespace_ref_count: 0,
        content_root: content_root(recipe.profile, recipe.seed, label),
        metadata_root: metadata_root(recipe.profile, recipe.seed, label),
    }
}

impl Recipe {
    /// Builds the input for this recipe.
    ///
    /// The result satisfies every ordering rule `FilesystemInput::check` applies,
    /// which `PreparedTree::check` asserts rather than assumes.
    pub fn prepare(&self) -> PreparedTree {
        let directories_count = self.directory_count();
        let mut directories: Vec<DirectoryUpdate> = Vec::new();
        let mut inodes: Vec<InodeUpdate> = vec![InodeUpdate {
            serial: ROOT_SERIAL,
            value: directory_value(self, "root"),
        }];
        let mut new_inodes: Vec<u64> = vec![ROOT_SERIAL];

        // The root's own bindings: one name per derived directory.
        let mut root_changes: Vec<(PathName, Option<u64>)> = Vec::new();
        for index in 0..directories_count {
            let serial = self.directory_serial(index);
            root_changes.push((name(&directory_name(index)), Some(serial)));
            inodes.push(InodeUpdate {
                serial,
                value: directory_value(self, &directory_name(index)),
            });
            new_inodes.push(serial);
        }
        root_changes.sort_by(|left, right| left.0.cmp(&right.0));
        directories.push(DirectoryUpdate {
            parent: ROOT_SERIAL,
            changes: root_changes,
        });

        // One block of files per directory, so the blocks tile `entries` exactly.
        let mut file_index = 0_u32;
        for index in 0..directories_count {
            let serial = self.directory_serial(index);
            let mut changes: Vec<(PathName, Option<u64>)> = Vec::new();
            for _ in 0..self.files_in(index) {
                let child = self.file_serial(file_index);
                let label = format!("{}/{}", directory_name(index), file_name(file_index));
                changes.push((name(&file_name(file_index)), Some(child)));
                inodes.push(InodeUpdate {
                    serial: child,
                    value: file_value(self, &label),
                });
                new_inodes.push(child);
                file_index += 1;
            }
            changes.sort_by(|left, right| left.0.cmp(&right.0));
            directories.push(DirectoryUpdate {
                parent: serial,
                changes,
            });
        }

        directories.sort_by_key(|update| update.parent);
        inodes.sort_by_key(|update| update.serial);
        new_inodes.sort_unstable();
        new_inodes.dedup();
        Self::assemble(*self, directories, inodes, new_inodes)
    }

    /// Wraps an already-built input and derives the views the oracle needs.
    ///
    /// A loaded artifact goes through this too, so the listing oracle is always a
    /// function of the input actually handed to the operation and never of the
    /// recipe that was supposed to produce it.
    pub fn assemble(
        recipe: Recipe,
        directories: Vec<DirectoryUpdate>,
        inodes: Vec<InodeUpdate>,
        new_inodes: Vec<u64>,
    ) -> PreparedTree {
        // Directory paths, by walking parents up to the root.
        let mut parent_of: BTreeMap<u64, u64> = BTreeMap::new();
        let mut name_of: BTreeMap<u64, String> = BTreeMap::new();
        for update in &directories {
            for (changed, binding) in &update.changes {
                if let Some(child) = binding {
                    parent_of.insert(*child, update.parent);
                    name_of.insert(*child, changed.as_str().to_string());
                }
            }
        }
        let path_of = |mut serial: u64| -> String {
            let mut parts: Vec<String> = Vec::new();
            while let Some(parent) = parent_of.get(&serial).copied() {
                let Some(step) = name_of.get(&serial) else {
                    break;
                };
                parts.push(step.clone());
                serial = parent;
                if parts.len() > 64 {
                    break;
                }
            }
            parts.reverse();
            parts.join("/")
        };

        let kinds: BTreeMap<u64, InodeKind> = inodes
            .iter()
            .map(|update| (update.serial, update.value.kind))
            .collect();

        let mut listings: Vec<(String, Vec<(String, u64)>)> = Vec::new();
        let mut files: Vec<(String, u64)> = Vec::new();
        let mut directory_serials: Vec<u64> = Vec::new();
        for update in &directories {
            let path = if update.parent == ROOT_SERIAL {
                String::new()
            } else {
                path_of(update.parent)
            };
            let mut entries: Vec<(String, u64)> = Vec::new();
            for (changed, binding) in &update.changes {
                if let Some(child) = binding {
                    entries.push((changed.as_str().to_string(), *child));
                }
            }
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            listings.push((path.clone(), entries));
            if update.parent == ROOT_SERIAL
                || kinds.get(&update.parent) == Some(&InodeKind::Directory)
            {
                directory_serials.push(update.parent);
            }
        }
        for update in &inodes {
            if update.value.kind == InodeKind::RegularFile {
                files.push((path_of(update.serial), update.serial));
            }
        }
        listings.sort_by(|left, right| left.0.cmp(&right.0));
        files.sort_by(|left, right| left.0.cmp(&right.0));
        directory_serials.push(ROOT_SERIAL);
        directory_serials.sort_unstable();
        directory_serials.dedup();

        PreparedTree {
            recipe,
            directories,
            inodes,
            new_inodes,
            listings,
            files,
            directory_serials,
        }
    }

    /// Builds the input directly, without the derived views.
    pub fn input_parts(&self) -> (Vec<DirectoryUpdate>, Vec<InodeUpdate>, Vec<u64>) {
        let prepared = self.prepare();
        (prepared.directories, prepared.inodes, prepared.new_inodes)
    }
}

impl PreparedTree {
    /// Total bindings the input states, which is what the walk ceiling charges.
    pub fn bindings(&self) -> usize {
        self.directories
            .iter()
            .map(|update| update.changes.len())
            .sum()
    }

    /// Serializes the input. The derived views are not written: they are
    /// recomputed on load, so the two cannot drift.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        out.push_str(FORMAT);
        out.push('\n');
        out.push_str(&format!("profile\t{}\n", self.recipe.profile));
        out.push_str(&format!("entries\t{}\n", self.recipe.entries));
        out.push_str(&format!("directories\t{}\n", self.recipe.directories));
        out.push_str(&format!("seed\t{}\n", self.recipe.seed));
        for update in &self.directories {
            for (changed, binding) in &update.changes {
                match binding {
                    Some(child) => out.push_str(&format!(
                        "binding\t{}\t{}\t{child}\n",
                        update.parent,
                        changed.as_str()
                    )),
                    None => out.push_str(&format!(
                        "absence\t{}\t{}\n",
                        update.parent,
                        changed.as_str()
                    )),
                }
            }
        }
        for update in &self.inodes {
            out.push_str(&format!(
                "inode\t{}\t{}\t{}\t{}\t{}\n",
                update.serial,
                update.value.kind.code(),
                update.value.namespace_ref_count,
                update.value.content_root,
                update.value.metadata_root
            ));
        }
        for serial in &self.new_inodes {
            out.push_str(&format!("new\t{serial}\n"));
        }
        out
    }

    /// Parses a serialized input and re-derives its views.
    pub fn from_text(text: &str) -> Result<Self, String> {
        let mut lines = text.lines();
        match lines.next() {
            Some(first) if first == FORMAT => {}
            other => return Err(format!("not a {FORMAT} artifact: {other:?}")),
        }
        let mut profile = "";
        let mut entries = 0_u32;
        let mut directories_count = 0_u32;
        let mut seed = 0_u64;
        let mut changes: BTreeMap<u64, Vec<(PathName, Option<u64>)>> = BTreeMap::new();
        let mut inodes: Vec<InodeUpdate> = Vec::new();
        let mut new_inodes: Vec<u64> = Vec::new();
        for line in lines {
            let fields: Vec<&str> = line.split('\t').collect();
            match fields.as_slice() {
                ["profile", value] => profile = Box::leak(value.to_string().into_boxed_str()),
                ["entries", value] => {
                    entries = value.parse().map_err(|_| format!("entries: {value}"))?
                }
                ["directories", value] => {
                    directories_count =
                        value.parse().map_err(|_| format!("directories: {value}"))?
                }
                ["seed", value] => seed = value.parse().map_err(|_| format!("seed: {value}"))?,
                ["binding", parent, changed, child] => {
                    let parent: u64 = parent.parse().map_err(|_| format!("parent: {parent}"))?;
                    let child: u64 = child.parse().map_err(|_| format!("child: {child}"))?;
                    changes.entry(parent).or_default().push((
                        PathName::new(changed).map_err(|_| format!("name: {changed}"))?,
                        Some(child),
                    ));
                }
                ["absence", parent, changed] => {
                    let parent: u64 = parent.parse().map_err(|_| format!("parent: {parent}"))?;
                    changes.entry(parent).or_default().push((
                        PathName::new(changed).map_err(|_| format!("name: {changed}"))?,
                        None,
                    ));
                }
                ["inode", serial, kind, refs, content, metadata] => {
                    let serial: u64 = serial.parse().map_err(|_| format!("serial: {serial}"))?;
                    let kind_code: u8 = kind.parse().map_err(|_| format!("kind: {kind}"))?;
                    let refs: u64 = refs.parse().map_err(|_| format!("refs: {refs}"))?;
                    let content: ObjectId =
                        content.parse().map_err(|_| format!("content: {content}"))?;
                    let metadata: ObjectId = metadata
                        .parse()
                        .map_err(|_| format!("metadata: {metadata}"))?;
                    inodes.push(InodeUpdate {
                        serial,
                        value: InodeValue {
                            kind: InodeKind::from_code(kind_code)
                                .map_err(|_| format!("kind code: {kind_code}"))?,
                            namespace_ref_count: refs,
                            content_root: content,
                            metadata_root: metadata,
                        },
                    });
                }
                ["new", serial] => {
                    new_inodes.push(serial.parse().map_err(|_| format!("new: {serial}"))?)
                }
                [] => {}
                other => return Err(format!("unrecognised line: {other:?}")),
            }
        }
        let directories: Vec<DirectoryUpdate> = changes
            .into_iter()
            .map(|(parent, mut list)| {
                list.sort_by(|left, right| left.0.cmp(&right.0));
                DirectoryUpdate {
                    parent,
                    changes: list,
                }
            })
            .collect();
        inodes.sort_by_key(|update| update.serial);
        new_inodes.sort_unstable();
        new_inodes.dedup();
        let recipe = Recipe {
            profile,
            entries,
            directories: directories_count,
            seed,
        };
        Ok(Recipe::assemble(recipe, directories, inodes, new_inodes))
    }

    /// Writes the artifact to `directory/prepared-tree.tsv`.
    pub fn emit(&self, directory: &Path) -> std::io::Result<std::path::PathBuf> {
        std::fs::create_dir_all(directory)?;
        let path = directory.join("prepared-tree.tsv");
        std::fs::write(&path, self.to_text())?;
        Ok(path)
    }

    /// Loads the artifact from `directory/prepared-tree.tsv`.
    pub fn load(directory: &Path) -> Result<Self, String> {
        let path = directory.join("prepared-tree.tsv");
        let text = std::fs::read_to_string(&path)
            .map_err(|error| format!("{}: {error}", path.display()))?;
        Self::from_text(&text)
    }

    /// Asserts the ordering rules `FilesystemInput::check` applies.
    pub fn check(&self) -> Result<(), String> {
        for update in &self.directories {
            if update.parent == 0 {
                return Err("directory parent 0".to_string());
            }
            if update.changes.windows(2).any(|pair| pair[0].0 >= pair[1].0) {
                return Err(format!(
                    "directory {} is not strictly sorted",
                    update.parent
                ));
            }
        }
        if self
            .directories
            .windows(2)
            .any(|pair| pair[0].parent >= pair[1].parent)
        {
            return Err("directories are not sorted by parent".to_string());
        }
        if self
            .inodes
            .windows(2)
            .any(|pair| pair[0].serial >= pair[1].serial)
        {
            return Err("inodes are not sorted by serial".to_string());
        }
        if self.new_inodes.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err("new_inodes are not sorted and unique".to_string());
        }
        Ok(())
    }
}
