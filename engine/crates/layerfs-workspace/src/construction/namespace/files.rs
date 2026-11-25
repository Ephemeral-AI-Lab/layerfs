//! One captured inode page's base files, decided and read together (#810 F5).
use super::inodes::Builder;
use super::records::add;
use crate::base::BaseFiles;
use crate::construction::captured::kept;
use crate::{OverlayCapturedNamespace, OverlayOperationRecords};
use layerfs_content::{
    object::inode_leaf::{InodeKind as CanonicalKind, InodeValue},
    ContentError, ContentResult,
};
use layerfs_overlay::{IndexedOperationRecordScope, Inode, InodeKind, OverlayError};

/// What one page's live base regular files need. `kept` are the serials
/// whose bytes the capture left as the base holds them, sorted: their base
/// content root is kept with no Store read. Every other one is opened with
/// its length from `files` and its root fetched with the page's.
#[derive(Default)]
pub(crate) struct Window {
    kept: Vec<u64>,
    pub files: BaseFiles,
}
impl Window {
    pub fn kept(&self, serial: u64) -> bool {
        self.kept.binary_search(&serial).is_ok()
    }
}

/// Decides and reads one page's base files. `bases` pairs each demanded
/// serial with its base record, in serial order. A file is kept only when
/// the overlay shows its every byte inherited (`kept::inherited`) and its
/// size is the base file's length: remembered for that immutable root, or
/// else asked of the provider with the files to open, in one demand.
/// The files to open are then fetched ahead in bounded batches.
pub(crate) fn window<P: OverlayCapturedNamespace + OverlayOperationRecords + ?Sized>(
    builder: &Builder<'_, '_, P>,
    rows: &[Inode],
    bases: &[(u64, Option<InodeValue>)],
) -> ContentResult<Window> {
    let (provider, shared, facts) = (builder.provider, builder.shared, builder.facts);
    let mut candidates = Vec::new();
    let mut demanded = bases.iter().peekable();
    for inode in rows {
        let Some((_, record)) = demanded.next_if(|(serial, _)| *serial == inode.serial) else {
            continue;
        };
        let file = inode.kind == InodeKind::File
            && inode.nlink != 0
            && inode.serial != facts.root_serial
            && !facts.reader.created_above(inode.born);
        if let (true, Some(value)) = (file, record) {
            if value.kind == CanonicalKind::RegularFile {
                candidates.push((inode.serial, inode.size, value.content_root));
            }
        }
    }
    if candidates.is_empty() {
        return Ok(Window::default());
    }
    // The checks each file's own binding makes before it reads anything.
    let route = builder.workspace.route();
    if route.engine() != facts.reader.capture().engine()
        || facts.operation.engine() != route.engine()
    {
        return Err(shared.borrow_mut().original(OverlayError::Stale.into()));
    }
    let client = builder.base.client();
    let mut kept = Vec::new();
    let mut unknown = Vec::new();
    let mut open = Vec::new();
    for (serial, size, root) in candidates {
        shared.borrow().ready()?;
        // A failed question is that file's own failure, kept as its custody
        // as a failed preparation of the file keeps it.
        let inherited = match kept::inherited(provider, facts.reader, serial, size) {
            Ok(inherited) => inherited,
            Err(original) => {
                let scope = IndexedOperationRecordScope {
                    owner: facts.operation,
                    file_scope: serial,
                };
                let custody = kept::refused(
                    provider,
                    facts.reader,
                    serial,
                    scope,
                    builder.base,
                    original,
                );
                return Err(shared.borrow_mut().file(custody));
            }
        };
        match (inherited, client.remembered_length(root)) {
            (true, Some(length)) if length == size => kept.push(serial),
            (true, None) => unknown.push((serial, size, root)),
            _ => open.push(root),
        }
    }
    // One length demand for every file still undecided or to be opened.
    let roots: Vec<_> = unknown
        .iter()
        .map(|(_, _, root)| *root)
        .chain(open.iter().copied())
        .collect();
    let lengths = if roots.is_empty() {
        Vec::new()
    } else {
        add(&mut shared.borrow_mut().work.base_file_demands, 1);
        client
            .provided_lengths(&roots)
            .map_err(|original| shared.borrow_mut().original(original))?
    };
    if lengths.len() != roots.len() {
        return Err(ContentError::BatchCardinality {
            requested: roots.len(),
            returned: lengths.len(),
        });
    }
    let mut lengths = roots.into_iter().zip(lengths);
    let mut opened = Vec::with_capacity(open.len());
    for ((serial, size, _), (root, length)) in unknown.into_iter().zip(lengths.by_ref()) {
        if length == size {
            kept.push(serial);
        } else {
            opened.push((root, length));
        }
    }
    opened.extend(lengths);
    kept.sort_unstable();
    add(&mut shared.borrow_mut().work.files_kept, kept.len() as u64);
    let files = builder
        .base
        .base_files(opened, builder.policy.capacities().whole_file_raw_limit)
        .map_err(|original| shared.borrow_mut().original(original))?;
    add(
        &mut shared.borrow_mut().work.base_file_prefetches,
        files.prefetch_batches,
    );
    Ok(Window { kept, files })
}
