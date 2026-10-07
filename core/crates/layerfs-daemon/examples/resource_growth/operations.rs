//! Real direct operations and independent bounded byte oracles.
use super::{allocation, bytes, produce};
use layerfs_content::{filesystem::PathName, ContentResult, FinalizedConsumer, FinalizedObject};
use layerfs_daemon::{bootstrap::OpenedStore, store::BoundWorkspace, Owner};
use layerfs_history::CommitStagedOutcome;
use layerfs_storage::Save;
use layerfs_telemetry::timer::Timing;
use std::{
    fs::File,
    io::{Read, Write},
};

pub fn commit(
    out: &mut File,
    opened: &OpenedStore,
    owner: &Owner,
    bound: &BoundWorkspace,
    index: usize,
) {
    let tag = 128 + index as u8;
    produce::mutate_tag(bound, tag);
    let before = opened.diagnostics().unwrap().writer;
    let local = owner.client().diagnostics().unwrap();
    let result = bound
        .commit(|save, _, snapshot| {
            produce::construct_tag(save, snapshot, opened.store.policy().construction(), tag)
        })
        .unwrap();
    let after = opened.diagnostics().unwrap().writer;
    let local_after = owner.client().diagnostics().unwrap();
    let record = match &result.history {
        CommitStagedOutcome::Committed(value) => value,
        other => panic!("{other:?}"),
    };
    let mut exact = result.captured.work().sql.expanded();
    exact.accumulate(result.installed.work().sql.expanded());
    let whole = local_after.sql_foreground.since(&local.sql_foreground);
    let d = result.storage;
    writeln!(out,"{{\"schema\":\"pre-s8-growth-operation-v1\",\"kind\":\"commit\",\"index\":{index},\"attempts\":1,\"root\":\"{}\",\"head\":\"{}\",\"writes\":{},\"reserve\":{},\"initial\":{},\"refills\":{},\"publish\":{},\"history\":1,\"pack_bytes\":{},\"sql_statements\":{},\"scan_steps\":{},\"engine_jobs\":{},\"engine_statements\":{}}}",record.root,record.id,after.write_commits-before.write_commits,d.reserve,d.initial_reservations,d.reservation_refills,d.publish,d.pack_write_bytes,after.statements-before.statements,after.fullscan_steps-before.fullscan_steps,local_after.admitted-local.admitted,whole.total().attempts).unwrap();
    assert_eq!(whole, exact);
    assert_eq!(local_after.admitted - local.admitted, 2);
    assert_eq!(
        after.write_commits - before.write_commits,
        d.reserve + d.publish + 1
    );
    assert_eq!(d.initial_reservations, 1);
    assert_eq!(d.reservation_refills, 0);
    drop(result);
    verify_name(bound, "entry-000000", &produce::changed_bytes_with_tag(tag));
}
pub fn verify_name(bound: &BoundWorkspace, name: &str, expected: &[u8]) {
    let op = bound.operation().unwrap();
    let base = op.workspace().base().unwrap();
    let inode = base
        .child(
            base.root().root_inode().serial(),
            &PathName::new(name).unwrap(),
        )
        .unwrap();
    let mut actual = Vec::new();
    base.plan_read(inode.serial, 0, 128)
        .unwrap()
        .emit(&mut actual)
        .unwrap();
    assert_eq!(actual, expected);
}
struct Input {
    at: u64,
    end: u64,
}
impl Read for Input {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        let n = (self.end - self.at).min(out.len() as u64) as usize;
        bytes::fill(bytes::Pattern::Dense, self.at, &mut out[..n]);
        self.at += n as u64;
        Ok(n)
    }
}
struct Tracked<'a, 'b> {
    save: &'a Save<'b>,
    pending: u64,
    pool: usize,
    live: u64,
    objects: u64,
}
impl FinalizedConsumer for Tracked<'_, '_> {
    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()> {
        self.save.sink().accept(object)?;
        self.pending = self.pending.max(self.save.pending_canonical_bytes());
        self.pool = self.pool.max(self.save.pooled_index_bytes());
        self.live = self.live.max(allocation::snapshot().0);
        self.objects += 1;
        Ok(())
    }
}
pub fn save(out: &mut File, opened: &OpenedStore, bound: &BoundWorkspace, length: u64) {
    let before = opened.diagnostics().unwrap().writer;
    let storage = opened.store.producer().unwrap();
    let save = storage.begin_save().unwrap();
    let policy = storage.policy().construction();
    let mut sink = Tracked {
        save: &save,
        pending: 0,
        pool: 0,
        live: 0,
        objects: 0,
    };
    let built = Timing::disabled("growth.save", |scope| {
        layerfs_content::construct_stream(
            policy,
            &policy.capacities(),
            Input { at: 0, end: length },
            &mut sink,
            scope.child("content"),
        )
    })
    .0
    .unwrap();
    let (pending, pool, live, objects) = (sink.pending, sink.pool, sink.live, sink.objects);
    let saved = save.finish().unwrap();
    let d = storage.diagnostics();
    let after = opened.diagnostics().unwrap().writer;
    writeln!(out,"{{\"schema\":\"pre-s8-growth-operation-v1\",\"kind\":\"save\",\"index\":0,\"attempts\":1,\"root\":\"{}\",\"bytes\":{length},\"writes\":{},\"reserve\":{},\"initial\":{},\"refills\":{},\"publish\":{},\"history\":0,\"pack_bytes\":{},\"canonical_bytes\":{},\"objects\":{objects},\"pending_observed_max\":{pending},\"pooled_observed_max\":{pool},\"rust_live_at_handoff_observed_max\":{live}}}",built.root,after.write_commits-before.write_commits,d.reserve,d.initial_reservations,d.reservation_refills,d.publish,d.pack_write_bytes,saved.canonical_bytes).unwrap();
    assert_eq!(
        after.write_commits - before.write_commits,
        d.reserve + d.publish
    );
    assert_eq!(d.initial_reservations, 1);
    assert!(pending <= layerfs_storage::policy::WAVE_CANONICAL_BYTES_LIMIT);
    drop(storage);
    super::observe::phase("verify");
    let op = bound.operation().unwrap();
    let mut oracle = bytes::Oracle::new(bytes::Pattern::Dense);
    while oracle.position < length {
        let end = (oracle.position + bytes::WINDOW as u64).min(length);
        Timing::disabled("growth.read", |scope| {
            layerfs_content::read_range(
                op.client(),
                built.root,
                oracle.position..end,
                &mut oracle,
                scope.child("read"),
            )
        })
        .0
        .unwrap();
    }
    assert_eq!(oracle.position, length);
    assert!(op.ports().failure().unwrap().is_none());
    writeln!(out,"{{\"schema\":\"pre-s8-growth-oracle-v1\",\"bytes\":{length},\"window\":{},\"full_match\":true}}",bytes::WINDOW).unwrap();
}
