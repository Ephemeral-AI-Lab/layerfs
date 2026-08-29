use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OperationId([u8; 16]);

impl OperationId {
    pub fn new() -> Self {
        static SERIAL: AtomicU64 = AtomicU64::new(0);
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&std::process::id().to_be_bytes());
        bytes.extend_from_slice(
            &std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
                .to_be_bytes(),
        );
        bytes.extend_from_slice(&SERIAL.fetch_add(1, Ordering::Relaxed).to_be_bytes());
        let digest = layerfs_content::ObjectId::for_bytes(&bytes).to_bytes();
        Self(digest[..16].try_into().expect("fixed operation id"))
    }
}

impl Default for OperationId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for OperationId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("o:")?;
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl std::str::FromStr for OperationId {
    type Err = crate::MonitorError;

    fn from_str(value: &str) -> crate::MonitorResult<Self> {
        Ok(Self(crate::route::parse_id(value, "o:")?))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationOutcome {
    Succeeded,
    Failed,
    Interrupted,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimingFragment {
    pub process_id: u32,
    pub started_ns: u64,
    pub elapsed_ns: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OperationReceipt {
    pub id: OperationId,
    pub name: String,
    pub outcome: OperationOutcome,
    pub queued_ns: u64,
    pub service_ns: u64,
    pub fragments: Vec<TimingFragment>,
    pub storage: Vec<layerfs_storage::StorageReceipt>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CliInvocationReceipt {
    pub operation_id: OperationId,
    pub outcome: OperationOutcome,
    pub total_elapsed_ns: u64,
    pub parse_elapsed_ns: u64,
    pub context_open_elapsed_ns: u64,
    pub operation_wait_elapsed_ns: u64,
    pub render_elapsed_ns: u64,
}

impl CliInvocationReceipt {
    pub fn timing_is_consistent(&self) -> bool {
        self.total_elapsed_ns
            >= self
                .parse_elapsed_ns
                .saturating_add(self.context_open_elapsed_ns)
                .saturating_add(self.operation_wait_elapsed_ns)
                .saturating_add(self.render_elapsed_ns)
    }

    pub fn to_json(&self) -> String {
        format!(
            "{{\"schema_version\":1,\"event\":\"invocation_receipt\",\"schema\":\"layerfs-cli-invocation-receipt-v1\",\"operation_id\":\"{}\",\"outcome\":\"{}\",\"total_elapsed_ns\":{},\"parse_elapsed_ns\":{},\"context_open_elapsed_ns\":{},\"operation_wait_elapsed_ns\":{},\"render_elapsed_ns\":{}}}",
            self.operation_id,
            format!("{:?}", self.outcome).to_lowercase(),
            self.total_elapsed_ns,
            self.parse_elapsed_ns,
            self.context_open_elapsed_ns,
            self.operation_wait_elapsed_ns,
            self.render_elapsed_ns,
        )
    }
}

impl OperationReceipt {
    pub fn timing_is_consistent(&self) -> bool {
        self.fragments.iter().all(|fragment| {
            fragment
                .started_ns
                .checked_add(fragment.elapsed_ns)
                .is_some()
        }) && self
            .fragments
            .iter()
            .map(|fragment| fragment.elapsed_ns)
            .sum::<u64>()
            <= self.service_ns.saturating_mul(64)
    }

    pub fn to_json(&self) -> String {
        format!(
            "{{\"schema\":\"layerfs-operation-receipt-v2\",\"operation_id\":\"{}\",\"operation\":\"{}\",\"outcome\":\"{}\",\"timing\":{{\"queue_elapsed_ns\":{},\"service_elapsed_ns\":{},\"fragments\":[{}]}},\"storage\":[{}]}}",
            self.id,
            escape(&self.name),
            format!("{:?}", self.outcome).to_lowercase(),
            self.queued_ns,
            self.service_ns,
            self.fragments
                .iter()
                .map(|fragment| format!(
                    "{{\"process_id\":{},\"started_ns\":{},\"elapsed_ns\":{}}}",
                    fragment.process_id, fragment.started_ns, fragment.elapsed_ns
                ))
                .collect::<Vec<_>>()
                .join(","),
            self.storage
                .iter()
                .map(storage_json)
                .collect::<Vec<_>>()
                .join(",")
        )
    }
}

fn storage_json(receipt: &layerfs_storage::StorageReceipt) -> String {
    match receipt {
        layerfs_storage::StorageReceipt::Local(receipt) => local_json(receipt),
        layerfs_storage::StorageReceipt::Transfer(receipt) => transfer_json(receipt),
    }
}

fn local_json(receipt: &layerfs_storage::LocalAdmissionReceipt) -> String {
    let facts = receipt
        .facts
        .iter()
        .map(|(kind, receipt)| {
            format!(
                "\"{}\":{{\"inserted_ids\":{},\"inserted_bytes\":{},\"reused_ids\":{},\"reused_bytes\":{}}}",
                format!("{kind:?}").to_lowercase(),
                receipt.inserted_ids,
                receipt.inserted_bytes,
                receipt.raced_existing_ids,
                receipt.raced_existing_bytes,
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"local\":{{\"candidate_coverage\":\"changed_set\",\"objects\":{{\"candidate_ids\":{},\"candidate_bytes\":{},\"inserted_ids\":{},\"inserted_bytes\":{},\"reused_ids\":{},\"reused_bytes\":{}}},\"facts\":{{{}}},\"database\":{{\"write_transactions\":{},\"rollback_transactions\":{},\"object_admission_transactions\":{},\"fact_admission_transactions\":{},\"visibility_transactions\":{},\"commit_sync_elapsed_ns\":{}}}}}}}",
        receipt.objects.candidate_ids,
        receipt.objects.candidate_bytes,
        receipt.objects.inserted_ids,
        receipt.objects.inserted_bytes,
        receipt.objects.reused_ids,
        receipt.objects.reused_bytes,
        facts,
        receipt.database.write_transactions,
        receipt.database.rollback_transactions,
        receipt.database.object_admission_transactions,
        receipt.database.fact_admission_transactions,
        receipt.database.visibility_transactions,
        receipt.database.commit_sync_elapsed_ns,
    )
}

fn transfer_json(receipt: &layerfs_storage::TransferReceipt) -> String {
    let facts = receipt
        .facts
        .iter()
        .map(|(kind, receipt)| {
            format!(
                "\"{}\":{}",
                format!("{kind:?}").to_lowercase(),
                set_json(*receipt)
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!(
        "{{\"objects\":{{\"candidate_coverage\":\"negotiated_frontier\",\"candidate_object_bytes\":null,{},\"known_subtrees_pruned\":{}}},\"facts\":{{{}}},\"database\":{{\"write_transactions\":{},\"rollback_transactions\":{},\"object_admission_transactions\":{},\"fact_admission_transactions\":{},\"visibility_transactions\":{},\"commit_sync_elapsed_ns\":{}}},\"transport\":{{\"object_membership_pages\":{},\"typed_membership_pages\":{},\"request_reply_turns\":{},\"one_way_payload_batches\":{},\"command_frames\":{},\"payload_frames\":{},\"reply_frames\":{},\"wire_bytes_sent\":null,\"wire_bytes_received\":null,\"peak_buffer_bytes\":{}}}}}",
        set_fields(receipt.objects.set),
        receipt.objects.known_subtrees_pruned,
        facts,
        receipt.database.write_transactions,
        receipt.database.rollback_transactions,
        receipt.database.object_admission_transactions,
        receipt.database.fact_admission_transactions,
        receipt.database.visibility_transactions,
        receipt.database.commit_sync_elapsed_ns,
        receipt.transport.object_membership_pages,
        receipt.transport.typed_membership_pages,
        receipt.transport.request_reply_turns,
        receipt.transport.one_way_payload_batches,
        receipt.transport.command_frames,
        receipt.transport.payload_frames,
        receipt.transport.reply_frames,
        receipt.transport.peak_buffer_bytes,
    )
}

fn set_json(receipt: layerfs_storage::TransferSetReceipt) -> String {
    format!("{{{}}}", set_fields(receipt))
}

fn set_fields(receipt: layerfs_storage::TransferSetReceipt) -> String {
    format!(
        "\"announced_ids\":{},\"preexisting_announced_ids\":{},\"missing_ids\":{},\"sent_ids\":{},\"sent_bytes\":{},\"inserted_ids\":{},\"inserted_bytes\":{},\"raced_existing_ids\":{},\"raced_existing_bytes\":{}",
        receipt.announced_ids,
        receipt.preexisting_announced_ids(),
        receipt.missing_ids,
        receipt.sent_ids,
        receipt.sent_bytes,
        receipt.inserted_ids,
        receipt.inserted_bytes,
        receipt.raced_existing_ids,
        receipt.raced_existing_bytes,
    )
}

fn escape(value: &str) -> String {
    value
        .chars()
        .flat_map(|character| match character {
            '"' => "\\\"".chars().collect::<Vec<_>>(),
            '\\' => "\\\\".chars().collect(),
            character if character.is_control() => "?".chars().collect(),
            character => vec![character],
        })
        .collect()
}
