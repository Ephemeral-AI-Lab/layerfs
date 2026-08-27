#![cfg(target_os = "macos")]

use layerfs_sdk::{
    BranchHead, BranchId, CommitResult, DirectCommitReceipt, IntegrityMode, LayerFs, LayerId,
    LayerStackId, NativeRoute, OperationRecordRef,
};
use std::fs;
use std::io::{Read, Write};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

const MIB: u64 = 1024 * 1024;

struct RepeatReader {
    byte: u8,
    remaining: u64,
}

impl Read for RepeatReader {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        let count = output.len().min(self.remaining as usize);
        output[..count].fill(self.byte);
        self.remaining -= count as u64;
        Ok(count)
    }
}

#[derive(Default)]
struct CountingSink(u64);

impl Write for CountingSink {
    fn write(&mut self, input: &[u8]) -> std::io::Result<usize> {
        self.0 += input.len() as u64;
        Ok(input.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[derive(Clone)]
struct Metric {
    mode: &'static str,
    name: &'static str,
    samples: Vec<u128>,
    bytes: Option<u64>,
}

impl Metric {
    fn p50(&self) -> u128 {
        let mut samples = self.samples.clone();
        samples.sort_unstable();
        samples[samples.len() / 2]
    }

    fn throughput(&self) -> Option<f64> {
        self.bytes
            .map(|bytes| bytes as f64 / MIB as f64 / (self.p50() as f64 / 1_000_000_000.0))
    }

    fn passes(&self) -> bool {
        match self.name {
            "A01" => self.throughput().is_some_and(|value| value >= 250.0),
            "A03a" | "A03b" => self.throughput().is_some_and(|value| value >= 150.0),
            "A04/logical" => self.p50() <= 15_000_000,
            "A04/native-edit-plus-checkpoint" => self.p50() <= 20_000_000,
            "A09" => self.throughput().is_some_and(|value| value >= 200.0),
            "A10" => self.throughput().is_some_and(|value| value >= 150.0),
            "A11" => self.p50() <= 5_000_000,
            "A12" => self.p50() <= 25_000_000,
            "A13" => self.p50() <= 4_000_000,
            _ => false,
        }
    }
}

#[test]
#[ignore = "release-only APFS scaling qualification"]
fn shipped_sdk_meets_apfs_scaling_and_latency_gates() {
    let size_mib = std::env::var("LAYERFS_QUALIFY_MIB")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(10);
    assert!((1..=100).contains(&size_mib));
    let bytes = size_mib * MIB;
    let base = fs::canonicalize(std::env::temp_dir())
        .unwrap()
        .join(format!(
            "layerfs-product-apfs-scaling-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    fs::create_dir(&base).unwrap();
    let df = std::process::Command::new("/bin/df")
        .arg("-P")
        .arg(&base)
        .output()
        .unwrap();
    assert!(df.status.success());
    let device = String::from_utf8(df.stdout)
        .unwrap()
        .lines()
        .nth(1)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .to_owned();
    let disk = std::process::Command::new("/usr/sbin/diskutil")
        .arg("info")
        .arg(device)
        .output()
        .unwrap();
    assert!(disk.status.success());
    assert!(String::from_utf8(disk.stdout)
        .unwrap()
        .lines()
        .any(|line| line.contains("File System Personality:   APFS")));
    let complete = Instant::now();
    let mut metrics = Vec::new();
    qualify_mode(
        &base.join("trusted"),
        IntegrityMode::TrustedLocalDev,
        "TrustedLocalDev",
        0x10,
        bytes,
        &mut metrics,
    );
    qualify_mode(
        &base.join("verified"),
        IntegrityMode::Verified,
        "Verified",
        0x20,
        bytes,
        &mut metrics,
    );
    let wall_ns = complete.elapsed().as_nanos();
    let enforce = size_mib == 100;
    if enforce {
        assert!(wall_ns <= 120_000_000_000, "campaign wall {wall_ns}");
        for metric in &metrics {
            assert!(metric.passes(), "{} {} failed", metric.mode, metric.name);
        }
    }
    let body = format!(
        "{{\n  \"schema\": \"layerfs-product-apfs-scaling-v1\",\n  \"status\": \"{}\",\n  \"source_commit\": \"{}\",\n  \"source_tree\": \"{}\",\n  \"size_mib\": {},\n  \"complete_wall_ns\": {},\n  \"metrics\": [\n{}\n  ]\n}}\n",
        if !enforce || metrics.iter().all(Metric::passes) {
            "PASS"
        } else {
            "REVISE"
        },
        std::env::var("LAYERFS_SOURCE_COMMIT").unwrap_or_else(|_| "UNBOUND".to_owned()),
        std::env::var("LAYERFS_SOURCE_TREE").unwrap_or_else(|_| "UNBOUND".to_owned()),
        size_mib,
        wall_ns,
        metrics
            .iter()
            .map(|metric| format!(
                "    {{\"mode\":\"{}\",\"name\":\"{}\",\"raw_ns\":{:?},\"p50_ns\":{},\"throughput_mib_s\":{},\"pass\":{}}}",
                metric.mode,
                metric.name,
                metric.samples,
                metric.p50(),
                metric
                    .throughput()
                    .map_or_else(|| "null".to_owned(), |value| format!("{value:.3}")),
                metric.passes(),
            ))
            .collect::<Vec<_>>()
            .join(",\n")
    );
    print!("{body}");
    if let Ok(output) = std::env::var("LAYERFS_QUALIFY_OUTPUT") {
        fs::write(output, &body).unwrap();
    }
    fs::remove_dir_all(base).unwrap();
}

fn qualify_mode(
    root: &std::path::Path,
    integrity: IntegrityMode,
    mode: &'static str,
    seed: u8,
    bytes: u64,
    metrics: &mut Vec<Metric>,
) {
    let fs = LayerFs::open(root, integrity).unwrap();
    let empty = fs.initialize_empty_root().unwrap();
    let stack = fs
        .create_layer_stack(
            LayerStackId::from_bytes([seed; 32]),
            LayerId::from_bytes([seed + 1; 32]),
            mode,
            empty,
        )
        .unwrap();
    let branch = fs
        .create_top_level_branch(BranchId::from_bytes([seed + 2; 32]), Some(mode), stack)
        .unwrap();

    let started = Instant::now();
    let mut import = fs.begin_direct(branch).unwrap();
    import
        .replace_file(
            "file",
            RepeatReader {
                byte: b'a',
                remaining: bytes,
            },
        )
        .unwrap();
    let (mut head, _) = accepted(import.commit().unwrap());
    metrics.push(metric(mode, "A03a", started, Some(bytes)));

    let version = fs.pin_branch_version(head).unwrap();
    let started = Instant::now();
    let mut sink = CountingSink::default();
    for offset in 0..bytes / MIB {
        fs.read_range(version, "file", offset * MIB..(offset + 1) * MIB, &mut sink)
            .unwrap();
    }
    assert_eq!(sink.0, bytes);
    metrics.push(metric(mode, "A01", started, Some(bytes)));

    let started = Instant::now();
    let mut replace = fs.begin_direct(head).unwrap();
    replace
        .replace_file(
            "file",
            RepeatReader {
                byte: b'b',
                remaining: bytes,
            },
        )
        .unwrap();
    let (next, record) = accepted(replace.commit().unwrap());
    head = next;
    metrics.push(metric(mode, "A03b", started, Some(bytes)));

    let started = Instant::now();
    let mut sink = CountingSink::default();
    fs.stream(fs.pin_branch_version(head).unwrap(), "file", &mut sink)
        .unwrap();
    assert_eq!(sink.0, bytes);
    metrics.push(metric(mode, "A09", started, Some(bytes)));

    let started = Instant::now();
    let mut cold = fs.begin_managed_materialization(head).unwrap();
    metrics.push(metric(mode, "A10", started, Some(bytes)));
    assert_eq!(cold.read("file", 0, 1).unwrap(), b"b");
    assert_eq!(cold.read("file", bytes - 1, 1).unwrap(), b"b");
    let started = Instant::now();
    let delta = cold
        .refresh_to(fs.pin_branch_version(head).unwrap())
        .unwrap();
    assert_eq!(delta.native.route, Some(NativeRoute::ExactNoop));
    assert_eq!(delta.native.bytes_read, 0);
    assert_eq!(delta.native.bytes_written, 0);
    assert_eq!(delta.rope.cdc_bytes_scanned, 0);
    assert_eq!(delta.authority_full_scans, 0);
    let receipt = cold.commit().unwrap();
    assert!(receipt.outcome.is_none());
    assert_eq!(receipt.refresh_counters, delta);
    metrics.push(metric(mode, "A11", started, None));

    let child = fs
        .create_child_branch(
            BranchId::from_bytes([seed + 3; 32]),
            Some("refresh-target"),
            record,
        )
        .unwrap();
    let mut target = fs.begin_direct(child).unwrap();
    target
        .replace_range("file", bytes / 2, 4096, std::io::Cursor::new([b'c'; 4096]))
        .unwrap();
    let (target, _) = accepted(target.commit().unwrap());
    let mut refresh = fs.begin_managed_materialization(head).unwrap();
    let started = Instant::now();
    let delta = refresh
        .refresh_to(fs.pin_branch_version(target).unwrap())
        .unwrap();
    assert!(matches!(
        delta.native.route,
        Some(NativeRoute::ClonePatch | NativeRoute::InPlacePatch)
    ));
    assert_eq!(delta.changed_paths, 1);
    assert_eq!(delta.full_fallback_files, 0);
    let receipt = refresh.commit().unwrap();
    assert!(receipt.cleanup.is_ok());
    assert!(receipt.timers.equation_closed);
    let (next, _) = accepted_result(receipt.outcome.unwrap());
    assert_eq!(next.root, target.root);
    head = next;
    metrics.push(metric(mode, "A12", started, None));

    let mut logical = Vec::new();
    for sample in 0..5_u8 {
        let mut operation = fs.begin_direct(head).unwrap();
        let started = Instant::now();
        operation
            .replace_range(
                "file",
                bytes / 3,
                4096,
                std::io::Cursor::new([seed + sample; 4096]),
            )
            .unwrap();
        let (next, _) = accepted(operation.commit().unwrap());
        logical.push(started.elapsed().as_nanos());
        head = next;
    }
    metrics.push(Metric {
        mode,
        name: "A04/logical",
        samples: logical,
        bytes: None,
    });

    let mut native = Vec::new();
    for sample in 0..5_u8 {
        let mut operation = fs.begin_managed_materialization(head).unwrap();
        let started = Instant::now();
        let counters = operation
            .managed_replace_range("file", bytes * 2 / 3, 4096, &[seed + 8 + sample; 4096])
            .unwrap();
        assert!(matches!(
            counters.native.route,
            Some(NativeRoute::ClonePatch | NativeRoute::InPlacePatch)
        ));
        assert_eq!(counters.full_fallback_files, 0);
        let receipt = operation.commit().unwrap();
        assert!(receipt.cleanup.is_ok());
        assert!(receipt.timers.equation_closed);
        let (next, _) = accepted_result(receipt.outcome.unwrap());
        native.push(started.elapsed().as_nanos());
        head = next;
    }
    metrics.push(Metric {
        mode,
        name: "A04/native-edit-plus-checkpoint",
        samples: native,
        bytes: None,
    });

    if mode == "TrustedLocalDev" {
        drop(fs);
        let mut opens = Vec::new();
        for _ in 0..11 {
            let started = Instant::now();
            let reopened = LayerFs::open(root, IntegrityMode::TrustedLocalDev).unwrap();
            assert_eq!(reopened.branch_head(head.branch_id).unwrap(), Some(head));
            opens.push(started.elapsed().as_nanos());
        }
        metrics.push(Metric {
            mode,
            name: "A13",
            samples: opens,
            bytes: None,
        });
    }
}

fn metric(mode: &'static str, name: &'static str, started: Instant, bytes: Option<u64>) -> Metric {
    Metric {
        mode,
        name,
        samples: vec![started.elapsed().as_nanos()],
        bytes,
    }
}

fn accepted(receipt: DirectCommitReceipt) -> (BranchHead, OperationRecordRef) {
    assert!(receipt.cleanup.is_ok());
    assert!(receipt.timers.equation_closed);
    accepted_result(receipt.outcome)
}

fn accepted_result(result: CommitResult) -> (BranchHead, OperationRecordRef) {
    match result {
        CommitResult::WorkingRecorded { head, record, .. } => (head, record),
        CommitResult::Conflict { .. } => panic!("qualification operation conflicted"),
    }
}
