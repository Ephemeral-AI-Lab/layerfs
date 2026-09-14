//! Optional physical maintenance against a completed published correspondence.
//! Logical contents, origin coordinates and inode revision never change here.
use crate::correspondence::{self, Description, Span};
use crate::host_overlay::HostOverlay;
use crate::overlay_budget::{Charge, Operation};
use crate::overlay_ranges::{OwnedPiece, Source};
use layerfs_content::file::content::{self, FileContentRoot};
use layerfs_layerstack_store::{CoreReader, Result, StoreError};
use layerfs_workspace_core::{Kind, NodeId};
use std::path::Path;

impl HostOverlay {
    /// The caller supplies only a completed, published description. Current
    /// Store branch/stage retention keeps admitted canonical objects alive;
    /// SnapshotReader keeps the Store open. Failed private-admission rollback
    /// cannot remove these already published objects.
    ///
    /// True means this comparison was processed, including unmatched later
    /// writes that must await their own publication. False means a newer root
    /// won installation and the caller must retain this pending work for retry.
    pub(crate) fn canonicalize_file(
        &self,
        node: NodeId,
        published: &Description,
        spool: &Path,
    ) -> Result<bool> {
        self.canonicalize_file_before_install(node, published, spool, || {})
    }

    fn canonicalize_file_before_install(
        &self,
        node: NodeId,
        published: &Description,
        spool: &Path,
        before_install: impl FnOnce(),
    ) -> Result<bool> {
        if published.inode != node {
            return Err(StoreError::InvalidInput("canonical maintenance inode"));
        }
        let canonical = published.canonical_root.ok_or(StoreError::InvalidInput(
            "canonical maintenance requires published content",
        ))?;
        // Two bounded journals can coexist. Leave allocation-block rounding
        // inside the reservation, rather than counting only their logical rows.
        let scratch = self.policy.overlay.max_scratch_bytes;
        let journal_limit = scratch
            .checked_sub(2 * 4096)
            .filter(|limit| *limit > 0)
            .ok_or(StoreError::InvalidInput(
                "canonical maintenance journal budget",
            ))?;
        let _permit = self.budget.enter(
            Operation::Prepare,
            Charge {
                memory_bytes: correspondence::PLAN_BUFFER_BYTES as u64,
                scratch_bytes: scratch,
                files: 2,
                ..Charge::default()
            },
        )?;
        let snapshot = self.snapshot()?;
        let Some((record, root)) = snapshot.inode_record(node)? else {
            return Ok(true);
        };
        if record.attr.kind != Kind::File {
            return Ok(true);
        }
        let original = self.ranges.restore(root)?;
        if original.len() != record.attr.size {
            return Err(StoreError::Integrity("canonical maintenance live length"));
        }
        if content::length(&CoreReader(&self.reader), FileContentRoot(canonical))?
            != published.length
        {
            return Err(StoreError::Integrity(
                "canonical maintenance published length",
            ));
        }
        if original.payload_bytes() == 0 && original.inline_bytes() == 0 {
            return Ok(true);
        }
        let mut cursor = self.ranges.cursor(&original, 0, original.len())?;
        let current = Description::build(
            &self.index,
            node,
            None,
            original.len(),
            original.count().max(1),
            std::iter::from_fn(|| cursor.next_descriptor().transpose()),
        )?;
        let plan = correspondence::plan(published, &current, spool, journal_limit)?;
        if plan.counters.journal_physical_bytes > scratch {
            return Err(StoreError::InvalidInput(
                "canonical maintenance allocated journal budget",
            ));
        }
        let mut replacement = original.clone();
        let mut changed = false;
        // Seek each proven span in the owned candidate range root. No payload
        // read, copied tree, origin rewrite or comparison of file bytes occurs.
        for span in plan.into_iter()? {
            let Span::Base {
                old_offset,
                new_offset,
                length,
            } = span?
            else {
                continue;
            };
            let end = new_offset
                .checked_add(length)
                .ok_or(StoreError::Integrity("canonical maintenance span"))?;
            let mut pieces = self.ranges.cursor(&replacement, new_offset, end)?;
            while let Some((position, piece, local, length)) = pieces.next_raw()? {
                if !matches!(piece.source, Source::Payload { .. } | Source::Inline { .. }) {
                    continue;
                }
                let stop = local
                    .checked_add(length)
                    .ok_or(StoreError::Integrity("canonical maintenance piece span"))?;
                if stop > piece.length {
                    return Err(StoreError::Integrity("canonical maintenance piece bounds"));
                }
                let boundaries = u64::from(local != 0) + u64::from(stop != piece.length);
                let count = replacement
                    .count()
                    .checked_add(boundaries)
                    .ok_or(StoreError::Integrity("canonical maintenance piece count"))?;
                if count > self.ranges.max_pieces() {
                    // Only a partial match needs extra boundaries. Such a piece
                    // changed after this published cut; leave its raw allocation
                    // charged until C2 captures its complete current interval.
                    // Whole pieces still proceed; actual replacement I/O/height
                    // or allocation failures below are never caught or hidden.
                    continue;
                }
                let offset = old_offset
                    .checked_add(position - new_offset)
                    .ok_or(StoreError::Integrity("canonical maintenance offset"))?;
                let canonical =
                    OwnedPiece::canonical_slice(piece, local, length, canonical, offset)?;
                replacement =
                    self.ranges
                        .replace(&replacement, position, length, [Ok(canonical)])?;
                changed = true;
            }
        }
        if !changed {
            return Ok(true);
        }
        let prepared = self.overlay.prepare(snapshot.root.clone(), |m| {
            m.cache_inode_record(record, replacement.root())
        })?;
        // No FIFO turn or root-install lock is held during planning or this
        // seam. Unrelated writes invalidate the whole leased source as required.
        before_install();
        self.overlay.install(prepared)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::changes::SnapshotCandidateFixture as Fixture;
    use crate::overlay_ranges::{Limits as RangeLimits, Ranges};
    use crate::snapshot::Snapshot;
    use crate::WorkspaceFileReplacement;
    use layerfs_content::{filesystem, CanonicalPath};
    use layerfs_workspace_core::ROOT;
    use std::sync::Arc;

    fn published_raw(length: usize) -> (Fixture, NodeId, Description, Snapshot) {
        let mut fixture = Fixture::new(length as u64);
        let node = fixture.host.lookup(ROOT, b"file").unwrap().node;
        fixture.host.write(node, 0, &vec![b'a'; length]).unwrap();
        let captured = fixture.host.snapshot().unwrap();
        let prepared = fixture.inputs(&captured).build(1).unwrap();
        fixture.publish(prepared);
        let description = fixture.comparison.description(node).unwrap().unwrap();
        (fixture, node, description, captured)
    }

    #[test]
    fn piece_cap_defers_partial_c1_backing_until_complete_c2_is_available() {
        let mut fixture = Fixture::new(4);
        // Use the real constructor cap on the same owned backing. One piece
        // faithfully exercises the8193-piece boundary without thousands of edits.
        fixture.host.ranges = Ranges::new(
            fixture.host.index.clone(),
            fixture.host.payload.clone(),
            RangeLimits {
                max_pieces: 1,
                ..RangeLimits::default()
            },
        )
        .unwrap();
        let node = fixture.host.lookup(ROOT, b"file").unwrap().node;
        fixture.host.write(node, 0, b"abcd").unwrap();
        let c1 = fixture.host.snapshot().unwrap();
        let prepared = fixture.inputs(&c1).build(1).unwrap();
        fixture.publish(prepared);
        let published = fixture.comparison.description(node).unwrap().unwrap();
        fixture.host.append(node, b"X").unwrap();
        let later = fixture.host.snapshot().unwrap();
        let tree = later.file_ranges(&fixture.host.ranges, node).unwrap();
        assert_eq!((tree.count(), tree.payload_bytes()), (1, 5));
        assert!(fixture
            .host
            .canonicalize_file(node, &published, &fixture.directory)
            .expect("optional C1 maintenance cannot add a boundary beyond the cap"));
        assert!(Arc::ptr_eq(
            &later.root,
            &fixture.host.snapshot().unwrap().root
        ));
        assert_eq!(
            fixture.host.payload.stats().unwrap().chargeable_bytes,
            5,
            "skipped live raw bytes remain charged"
        );
        fixture.comparison.maintenance_complete(node).unwrap();
        assert!(!fixture
            .comparison
            .maintenance_nodes(None)
            .unwrap()
            .contains(&node));
        let mut bytes = [0; 5];
        fixture.host.read_into(node, 0, &mut bytes).unwrap();
        assert_eq!(&bytes, b"abcdX");
        let prepared = fixture.inputs(&later).build(1).unwrap();
        assert_eq!(prepared.diagnostics.replacement_bytes, 1);
        fixture.publish(prepared);
        assert!(
            fixture
                .comparison
                .maintenance_nodes(None)
                .unwrap()
                .contains(&node),
            "the changed file is requeued by its C2 publication"
        );
        let published = fixture.comparison.description(node).unwrap().unwrap();
        assert!(fixture
            .host
            .canonicalize_file(node, &published, &fixture.directory)
            .unwrap());
        let current = fixture.host.snapshot().unwrap();
        let canonical = current.file_ranges(&fixture.host.ranges, node).unwrap();
        assert_eq!((canonical.count(), canonical.payload_bytes()), (1, 0));
        assert_eq!(fixture.host.ranges.max_pieces(), 1);
        assert_eq!(current.root.sequence, later.root.sequence);
        current
            .read_at(
                &fixture.host.ranges,
                &fixture.host.reader,
                node,
                0,
                &mut bytes,
            )
            .unwrap();
        assert_eq!(&bytes, b"abcdX");
        assert_eq!(
            c1.read_at(
                &fixture.host.ranges,
                &fixture.host.reader,
                node,
                0,
                &mut bytes
            )
            .unwrap(),
            4
        );
        assert_eq!(&bytes[..4], b"abcd");
        drop((c1, later, tree));
        let mut steps = 0;
        while fixture.host.maintain().unwrap() {
            steps += 1;
            assert!(steps < 10_000);
        }
        assert_eq!(fixture.host.payload.stats().unwrap().chargeable_bytes, 0);
        assert_eq!(fixture.host.payload.stats().unwrap().physical_bytes, 0);
        println!("piece cap remains1: C1 matched prefix stays5B charged; admitted C2 convertswholepiece; raw physical reaches0");
    }

    #[test]
    fn whole_piece_substitution_proceeds_when_neighbor_prefix_hits_cap() {
        let mut fixture = Fixture::new(4);
        fixture.host.ranges = Ranges::new(
            fixture.host.index.clone(),
            fixture.host.payload.clone(),
            RangeLimits {
                max_pieces: 2,
                ..RangeLimits::default()
            },
        )
        .unwrap();
        let node = fixture.host.lookup(ROOT, b"file").unwrap().node;
        fixture.host.write(node, 0, b"abcd").unwrap();
        fixture
            .host
            .edit_many(
                node,
                &[(0, 1, WorkspaceFileReplacement::Inline(vec![b'I']))],
            )
            .unwrap();
        let c1 = fixture.host.snapshot().unwrap();
        let prepared = fixture.inputs(&c1).build(1).unwrap();
        fixture.publish(prepared);
        fixture.host.append(node, b"X").unwrap();
        let published = fixture.comparison.description(node).unwrap().unwrap();
        assert!(fixture
            .host
            .canonicalize_file(node, &published, &fixture.directory)
            .unwrap());
        let partial = fixture.host.snapshot().unwrap();
        let tree = partial.file_ranges(&fixture.host.ranges, node).unwrap();
        assert_eq!(
            (tree.count(), tree.payload_bytes(), tree.inline_bytes()),
            (2, 4, 0),
            "whole inline piece converts while only the split-requiring payload prefix waits"
        );
        fixture.comparison.maintenance_complete(node).unwrap();
        let prepared = fixture.inputs(&partial).build(1).unwrap();
        fixture.publish(prepared);
        assert!(fixture
            .comparison
            .maintenance_nodes(None)
            .unwrap()
            .contains(&node));
        let published = fixture.comparison.description(node).unwrap().unwrap();
        assert!(fixture
            .host
            .canonicalize_file(node, &published, &fixture.directory)
            .unwrap());
        let current = fixture.host.snapshot().unwrap();
        let canonical = current.file_ranges(&fixture.host.ranges, node).unwrap();
        assert_eq!((canonical.count(), canonical.payload_bytes()), (2, 0));
        assert_eq!(fixture.host.ranges.max_pieces(), 2);
        let mut bytes = [0; 5];
        current
            .read_at(
                &fixture.host.ranges,
                &fixture.host.reader,
                node,
                0,
                &mut bytes,
            )
            .unwrap();
        assert_eq!(&bytes, b"IbcdX");
        assert_eq!(
            c1.read_at(
                &fixture.host.ranges,
                &fixture.host.reader,
                node,
                0,
                &mut bytes
            )
            .unwrap(),
            4
        );
        assert_eq!(&bytes[..4], b"Ibcd");
        drop((c1, partial, tree));
        let mut steps = 0;
        while fixture.host.maintain().unwrap() {
            steps += 1;
            assert!(steps < 10_000);
        }
        assert_eq!(fixture.host.payload.stats().unwrap().physical_bytes, 0);
    }

    fn check_raw_substitution(length: usize) -> (Fixture, NodeId) {
        let (fixture, node, published, captured) = published_raw(length);
        let host = &fixture.host;
        let middle = length as u64 / 2;
        host.write(node, middle, b"X").unwrap();
        let before = host.snapshot().unwrap();
        let record = before.inode_record(node).unwrap().unwrap().0;
        assert!(host
            .canonicalize_file(node, &published, &fixture.directory)
            .unwrap());
        let current = host.snapshot().unwrap();
        assert_eq!(current.inode_record(node).unwrap().unwrap().0, record);
        assert_eq!(current.root.sequence, before.root.sequence);
        assert_eq!(
            current.root.live_inline_bytes,
            before.root.live_inline_bytes
        );
        let tree = current.file_ranges(&host.ranges, node).unwrap();
        assert_eq!(tree.payload_bytes(), 1);
        let mut cursor = host.ranges.cursor(&tree, 0, tree.len()).unwrap();
        let mut sources = Vec::new();
        while let Some((descriptor, base)) = cursor.next_base_descriptor().unwrap() {
            sources.push((descriptor.offset, descriptor.length, base));
        }
        let canonical = published.canonical_root.unwrap();
        assert_eq!(
            sources,
            vec![
                (0, middle, Some((canonical, 0))),
                (middle, 1, None),
                (
                    middle + 1,
                    length as u64 - middle - 1,
                    Some((canonical, middle + 1))
                ),
            ]
        );
        let mut bytes = [0; 3];
        current
            .read_at(&host.ranges, &host.reader, node, middle - 1, &mut bytes)
            .unwrap();
        assert_eq!(&bytes, b"aXa");
        captured
            .read_at(&host.ranges, &host.reader, node, middle - 1, &mut bytes)
            .unwrap();
        assert_eq!(&bytes, b"aaa");
        let old_allocation = host.payload.stats().unwrap().physical_bytes;
        assert!(
            old_allocation >= length as u64,
            "old snapshot still owns raw source"
        );
        drop((before, captured));
        let mut steps = 0;
        while host.maintain().unwrap() {
            steps += 1;
            assert!(steps < 50_000, "bounded payload cleanup did not finish");
        }
        let reclaimed = host.payload.stats().unwrap();
        assert!(reclaimed.physical_bytes <= 4096, "{reclaimed:?}");
        assert_eq!(reclaimed.chargeable_bytes, 1);
        assert!(host
            .canonicalize_file(node, &published, &fixture.directory)
            .unwrap());
        assert!(
            Arc::ptr_eq(&current.root, &host.snapshot().unwrap().root),
            "already processed correspondence must not replace the live root"
        );
        current
            .read_at(&host.ranges, &host.reader, node, middle - 1, &mut bytes)
            .unwrap();
        assert_eq!(&bytes, b"aXa");
        println!("canonical substitution: captured={length}B, live raw=1B, physical {old_allocation}->{}B, cleanup steps={steps}", reclaimed.physical_bytes);
        (fixture, node)
    }

    #[test]
    fn published_ranges_replace_only_proven_raw_bytes_around_later_edit() {
        check_raw_substitution(256 * 1024);
    }

    #[test]
    fn sixty_four_mib_capture_reclaims_to_one_later_write_block() {
        const LENGTH: usize = 64 * 1024 * 1024;
        let (mut fixture, _) = check_raw_substitution(LENGTH);
        let captured = fixture.host.snapshot().unwrap();
        let prepared = fixture.inputs(&captured).build(1).unwrap();
        let diagnostics = prepared.diagnostics;
        assert_eq!(diagnostics.replacement_bytes, 1);
        assert_eq!(diagnostics.reused_bytes, LENGTH as u64 - 1);
        assert_eq!(diagnostics.full_comparisons, 0);
        assert_eq!(diagnostics.full_builds, 0);
        let (root, _) = fixture.publish(prepared);
        let reader = fixture.store.snapshot_reader(root);
        let file = filesystem::resolve(
            &CoreReader(&reader),
            root,
            &CanonicalPath::new("file").unwrap(),
            &mut filesystem::LogicalCounters::default(),
        )
        .unwrap();
        let mut bytes = Vec::new();
        let middle = LENGTH as u64 / 2;
        content::read_range(
            &CoreReader(&reader),
            FileContentRoot(file.record.content_root),
            middle - 1..middle + 2,
            &mut bytes,
        )
        .unwrap();
        assert_eq!(bytes, b"aXa");
        println!("C2 after canonical substitution: {diagnostics:?}");
    }

    #[test]
    fn stale_substitution_preserves_unrelated_write_and_retries_leased_root() {
        let (fixture, node, published, captured) = published_raw(4096);
        let host = &fixture.host;
        let unrelated = host.create_file(ROOT, b"unrelated", 0o600).unwrap().node;
        assert!(!host
            .canonicalize_file_before_install(node, &published, &fixture.directory, || {
                host.write(unrelated, 0, b"new").unwrap();
            })
            .unwrap());
        let current = host.snapshot().unwrap();
        assert_eq!(
            current
                .file_ranges(&host.ranges, node)
                .unwrap()
                .payload_bytes(),
            4096
        );
        let mut bytes = [0; 3];
        host.read_into(unrelated, 0, &mut bytes).unwrap();
        assert_eq!(&bytes, b"new");
        assert!(host
            .canonicalize_file(node, &published, &fixture.directory)
            .unwrap());
        assert_eq!(
            host.snapshot()
                .unwrap()
                .file_ranges(&host.ranges, node)
                .unwrap()
                .payload_bytes(),
            0
        );
        host.read_into(unrelated, 0, &mut bytes).unwrap();
        assert_eq!(&bytes, b"new");
        captured
            .read_at(&host.ranges, &host.reader, node, 0, &mut bytes)
            .unwrap();
        assert_eq!(&bytes, b"aaa");
    }

    #[test]
    fn canonical_inline_substitution_releases_only_current_equivalent_inline_charge() {
        let mut fixture = Fixture::new(64);
        let host = &fixture.host;
        let node = host.lookup(ROOT, b"file").unwrap().node;
        host.edit_many(
            node,
            &[(0, 64, WorkspaceFileReplacement::Inline(vec![b'i'; 64]))],
        )
        .unwrap();
        let captured = host.snapshot().unwrap();
        let prepared = fixture.inputs(&captured).build(1).unwrap();
        fixture.publish(prepared);
        let host = &fixture.host;
        let published = fixture.comparison.description(node).unwrap().unwrap();
        host.edit_many(
            node,
            &[(1, 1, WorkspaceFileReplacement::Inline(vec![b'X']))],
        )
        .unwrap();
        let before = host.snapshot().unwrap();
        assert_eq!(before.root.live_inline_bytes, 64);
        let mut invalid = published.clone();
        invalid.length += 1;
        assert!(host
            .canonicalize_file(node, &invalid, &fixture.directory)
            .is_err());
        assert!(Arc::ptr_eq(&before.root, &host.snapshot().unwrap().root));
        assert!(host
            .canonicalize_file(node, &published, &fixture.directory)
            .unwrap());
        let current = host.snapshot().unwrap();
        assert_eq!(current.root.live_inline_bytes, 1);
        assert_eq!(current.root.sequence, before.root.sequence);
        assert_eq!(
            current.inode_record(node).unwrap().unwrap().0,
            before.inode_record(node).unwrap().unwrap().0
        );
        let mut bytes = [0; 3];
        current
            .read_at(&host.ranges, &host.reader, node, 0, &mut bytes)
            .unwrap();
        assert_eq!(&bytes, b"iXi");
        captured
            .read_at(&host.ranges, &host.reader, node, 0, &mut bytes)
            .unwrap();
        assert_eq!(&bytes, b"iii");
    }
}
