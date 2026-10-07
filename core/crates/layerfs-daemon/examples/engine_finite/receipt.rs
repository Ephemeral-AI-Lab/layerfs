//! Fresh bounded original-job stream and its exact aggregate reconciliation.
use super::{json::Json, records};
use layerfs_daemon::{Completion, OwnerWork, ServiceClass};
use layerfs_overlay::{AllocationWork, DatabaseWork, PayloadWork};
use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::Path,
};
pub struct Recorder {
    output: File,
    pub jobs: u64,
    pub classes: [u64; 6],
    pub wave_classes: [[u64; 6]; 4],
    sql: DatabaseWork,
    payload: PayloadWork,
    allocation: AllocationWork,
}
impl Recorder {
    pub fn new(path: &Path) -> Self {
        Self {
            output: OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .unwrap(),
            jobs: 0,
            classes: [0; 6],
            wave_classes: [[0; 6]; 4],
            sql: DatabaseWork::default(),
            payload: PayloadWork::default(),
            allocation: AllocationWork::default(),
        }
    }
    pub fn job(&mut self, workspace: usize, class: ServiceClass, phase: &str, done: &Completion) {
        let work = done.work();
        let sql = work.sql.expanded();
        self.sql.accumulate(sql);
        self.payload.accumulate(work.payload);
        self.allocation.accumulate(work.allocation);
        self.classes[class as usize] += 1;
        if phase == "wave" {
            self.wave_classes[workspace][class as usize] += 1;
        }
        let mut out = Json::new();
        out.raw("{\"schema\":\"cluster-two-engine-job-v1\",\"mode\":\"diagnostic\",\"attempt_count\":1,").unwrap();
        out.field("index", self.jobs).unwrap();
        out.raw(",").unwrap();
        out.field("workspace", workspace).unwrap();
        out.raw(",").unwrap();
        out.field("class", class as usize).unwrap();
        out.raw(",").unwrap();
        out.text("phase", phase).unwrap();
        out.raw(",").unwrap();
        out.text(
            "outcome",
            if done.result().is_ok() {
                "known_success"
            } else {
                "original_failure"
            },
        )
        .unwrap();
        out.raw(",\"sql\":").unwrap();
        records::database(&mut out, &sql).unwrap();
        out.raw(",\"payload\":").unwrap();
        records::payload(&mut out, work.payload).unwrap();
        out.raw(",\"allocation\":").unwrap();
        records::allocation(&mut out, work.allocation).unwrap();
        out.raw(",").unwrap();
        out.field("parked_turns", work.parked_turns).unwrap();
        out.raw(",").unwrap();
        out.field("queue_wait_including_parking_ns", work.queue_wait_ns)
            .unwrap();
        out.raw(",").unwrap();
        out.field("service_inclusive_ns", work.service_ns).unwrap();
        out.raw("}").unwrap();
        self.output.write_all(&out.finish().unwrap()).unwrap();
        self.jobs += 1;
        assert!(done.result().is_ok(), "original outcome: {done:?}");
    }
    pub fn reconcile(&self, work: OwnerWork) {
        assert_eq!(self.sql, work.sql_foreground);
        assert_eq!(self.payload, work.payload_foreground);
        assert_eq!(self.allocation, work.allocation_foreground);
        assert_eq!(self.classes, work.completed);
        assert_eq!(self.jobs, work.admitted);
        assert_eq!(work.outstanding, 0);
        assert_eq!(work.credited_bytes, 0);
        assert_eq!(work.receipt_overruns, 0);
        assert!(self.wave_classes.iter().all(|counts| *counts == [32; 6]));
    }
    pub fn finish(&mut self, work: OwnerWork) {
        let mut out = Json::new();
        out.raw("{\"schema\":\"cluster-two-engine-summary-v1\",\"mode\":\"diagnostic\",\"numeric_acceptance\":\"OWNER_DEFERRED\",\"workspaces\":4,\"classes\":6,\"waves\":32,").unwrap();
        out.field("jobs", self.jobs).unwrap();
        out.raw(",\"owner\":").unwrap();
        records::owner(&mut out, work).unwrap();
        out.raw(",").unwrap();
        out.field("closed_namespaces", work.closed_namespaces)
            .unwrap();
        out.raw(",").unwrap();
        out.field("peak_queued", work.peak_queued).unwrap();
        out.raw(",").unwrap();
        out.field("scheduler_bytes", work.scheduler_bytes).unwrap();
        out.raw(",\"wave_counts\":[").unwrap();
        for (i, counts) in self.wave_classes.iter().enumerate() {
            if i > 0 {
                out.raw(",").unwrap();
            }
            out.raw("[").unwrap();
            for (j, value) in counts.iter().enumerate() {
                if j > 0 {
                    out.raw(",").unwrap();
                }
                out.raw(&value.to_string()).unwrap();
            }
            out.raw("]").unwrap();
        }
        out.raw("]}").unwrap();
        self.output.write_all(&out.finish().unwrap()).unwrap();
    }
}
