use crate::stage1_fixture::BUFFER_BYTES;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::AtomicU64;
use std::time::Instant;
pub(crate) const RESET_COUNT: u64 = 54;
pub(crate) const RESET_RESERVE_NS: u128 = 15_000_000_000;
pub(crate) const RESET_LIMIT_NS: u128 = 5_000_000_000;
pub(crate) const CAMPAIGN_LIMIT_NS: u128 = 120_000_000_000;
pub(crate) const MIB: u64 = 1_048_576;
pub(crate) static READINESS_ARTIFACT_SERIAL: AtomicU64 = AtomicU64::new(0);
// Frozen poc/14 upper estimates, excluding clone resets. These are forecast
// inputs, never measured results.
pub(crate) const FORECAST_READ_NS: u128 = 7_000_000_000;
pub(crate) const FORECAST_WRITE_NS: u128 = 8_000_000_000;
pub(crate) const FORECAST_EDIT_NS: u128 = 18_000_000_000;
pub(crate) const FORECAST_MANAGED_NS: u128 = 10_000_000_000;
pub(crate) const FORECAST_MISC_NS: u128 = 9_000_000_000;
pub(crate) const FORECAST_POSTCHECK_ARTIFACT_NS: u128 = 8_000_000_000;
pub(crate) const STORE_PAGE_SIZE: i64 = 4_096;
pub(crate) const STORE_CACHE_PAGES: i64 = 1_280;
pub(crate) const STORE_CACHE_SPILL_PAGES: i64 = 1_280;
#[derive(Clone, Debug)]
pub(crate) struct Environment {
    pub(crate) git_commit: String,
    pub(crate) dirty_tree_blake3: String,
    pub(crate) source_tree_blake3: String,
    pub(crate) source_file_count: u64,
    pub(crate) source_files: Vec<String>,
    pub(crate) cargo_lock_blake3: String,
    pub(crate) executable_blake3: String,
    pub(crate) build_profile: &'static str,
    pub(crate) debug_assertions: bool,
    pub(crate) uname: String,
    pub(crate) macos: String,
    pub(crate) apfs_identity: String,
}
#[derive(Clone, Debug)]
pub(crate) struct Readiness {
    pub(crate) environment: Environment,
    pub(crate) master_digest: String,
    pub(crate) reset_observations_ns: Vec<u128>,
    pub(crate) reset_upper_ns: u128,
    pub(crate) forecast_reset_wall_ns: u128,
    pub(crate) forecast_campaign_wall_ns: u128,
    pub(crate) apfs_identity: String,
    pub(crate) store_database_bytes: BTreeMap<String, u64>,
    pub(crate) store_sqlite_profile: StoreSqliteProfile,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct StoreSqliteProfile {
    pub(crate) page_size: i64,
    pub(crate) cache_pages: i64,
    pub(crate) cache_spill_pages: i64,
}
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct EngineDelta {
    pub(crate) integrity_mode: crate::legacy_full::IntegrityMode,
    pub(crate) transactions_started: u64,
    pub(crate) transactions_committed: u64,
    pub(crate) transactions_rolled_back: u64,
    pub(crate) statements: u64,
    pub(crate) objects_validated: u64,
    pub(crate) objects_created: u64,
    pub(crate) objects_reused: u64,
    pub(crate) object_bytes_read: u64,
    pub(crate) object_bytes_written: u64,
    pub(crate) range_bytes_requested: u64,
    pub(crate) range_bytes_returned: u64,
    pub(crate) root_verifications: u64,
    pub(crate) root_verification_objects: u64,
    pub(crate) root_verification_bytes: u64,
    pub(crate) fetched_rows: u64,
    pub(crate) fetched_row_authentication_passes: u64,
    pub(crate) fetched_row_role_decode_passes: u64,
    pub(crate) new_object_authentication_passes: u64,
    pub(crate) incumbent_authentication_passes: u64,
    pub(crate) payload_batch_queries: u64,
    pub(crate) payload_batch_references: u64,
    pub(crate) payload_batch_session_maximum: u64,
    pub(crate) put_lookup_statements: u64,
    pub(crate) put_insert_statements: u64,
    pub(crate) created_rows: u64,
    pub(crate) reused_rows: u64,
    pub(crate) publication_commits: u64,
    pub(crate) publication_closure_passes: u64,
    pub(crate) namespace_graph_verification_passes: u64,
    pub(crate) scratch_tables: u64,
    pub(crate) scratch_statements: u64,
    pub(crate) scratch_rows: u64,
    pub(crate) scratch_session_high_water_bytes: u64,
}
#[derive(Default)]
pub(crate) struct DigestSink {
    pub(crate) hasher: blake3::Hasher,
    pub(crate) bytes: u64,
}
impl Write for DigestSink {
    fn write(&mut self, input: &[u8]) -> std::io::Result<usize> {
        if input.len() > BUFFER_BYTES {
            return Err(std::io::Error::other(
                "product emitted a stream buffer larger than 1 MiB",
            ));
        }
        self.hasher.update(input);
        self.bytes = self
            .bytes
            .checked_add(input.len() as u64)
            .ok_or_else(|| std::io::Error::other("digest byte counter overflow"))?;
        Ok(input.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl DigestSink {
    pub(crate) fn finish(self) -> (u64, String) {
        (self.bytes, self.hasher.finalize().to_hex().to_string())
    }
}
pub(crate) struct BoundedRead<R>(pub(crate) R);
impl<R: Read> Read for BoundedRead<R> {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        if output.len() > BUFFER_BYTES {
            return Err(std::io::Error::other(
                "product requested a stream buffer larger than 1 MiB",
            ));
        }
        self.0.read(output)
    }
}
#[derive(Clone, Debug)]
pub(crate) struct EditCase {
    pub(crate) id: &'static str,
    pub(crate) base: &'static str,
    pub(crate) base_len: u64,
    pub(crate) start: u64,
    pub(crate) delete_len: u64,
    pub(crate) replacement: Vec<u8>,
}
#[derive(Default)]
pub(crate) struct CampaignData {
    pub(crate) reset_count: u64,
    pub(crate) reset_wall_ns: u128,
    pub(crate) open_wall_ns: u128,
    pub(crate) managed_prepare_wall_ns: u128,
    pub(crate) operation_wall_ns: u128,
    pub(crate) postcheck_wall_ns: u128,
    pub(crate) cleanup_wall_ns: u128,
    pub(crate) artifact_wall_ns: u128,
    pub(crate) metrics: BTreeMap<String, Vec<u128>>,
    pub(crate) bytes_per_observation: BTreeMap<String, u64>,
    pub(crate) output_roots: BTreeMap<String, String>,
    pub(crate) last_q_terminal_bytes: Option<u64>,
    pub(crate) store_database_bytes_max: Option<u64>,
    pub(crate) process_resources: Vec<ProcessResources>,
}
#[derive(Clone, Debug)]
pub(crate) struct ProcessResources {
    pub(crate) operation: String,
    pub(crate) observed: bool,
    pub(crate) current_rss_bytes: u64,
    pub(crate) process_peak_rss_bytes: u64,
}
pub(crate) struct Campaign<'a> {
    pub(crate) run: &'a Path,
    pub(crate) started: Instant,
    pub(crate) rows: File,
    pub(crate) data: &'a mut CampaignData,
}
