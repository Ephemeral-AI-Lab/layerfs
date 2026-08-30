#![forbid(unsafe_code)]

mod admission;
mod error;
mod ids;
mod records;
mod schema;
mod sql;
mod transfer;
mod wire;

pub use admission::{
    apply_changes, apply_reconcile_choices, collect_dependency_set, dependency_order, empty_root,
    reconcile_candidate, reconcile_candidate_with, BuildCounters, BuiltRoot,
    CandidateReconciliation, CoreReader, DeferredObjectStore, ObjectBuffer, SpillableObjectSet,
};
pub use error::{Result, StorageError};
pub use ids::*;
pub use layerfs_content::filesystem::{DiffAspects, DiffEntry, NodeSummary};
pub use layerfs_content::filesystem::{ReconcileChoice, ReconcileConflict, ReconcileConflictKind};
pub use records::*;
pub use schema::*;
pub(crate) use transfer::{note_receiver_authentication, note_traversal_authentication};
pub use transfer::{
    receipt_totals, record_local_admission, take_storage_receipts, take_transfer_receipts,
    transfer_facts, transfer_root, transfer_roots, AdmissionSetReceipt, CanonicalObject,
    EndpointTarget, LayerStackEndpoint, LocalAdmissionReceipt, LocalObjectReceipt, MeasuredBytes,
    MissingBitmap, ObjectSource, RootTransferRequest, StorageReceipt, TransferPipeline,
    TransferReceipt, TransferSetReceipt, TransferTarget, FACT_BATCH_BYTES, FACT_BATCH_COUNT,
    ID_BATCH_COUNT, OBJECT_BATCH_BYTES, OBJECT_BATCH_COUNT, TRANSFER_BUFFER_BYTES,
};
#[cfg(feature = "test-instrumentation")]
pub use transfer::{reset_transfer_authentication_counts, transfer_authentication_counts};
pub use wire::{
    decode_diff_entry, decode_fact, decode_objects, encode_diff_entry, encode_fact, encode_objects,
    read_frame, write_frame,
};
