#![forbid(unsafe_code)]

mod admission;
mod candidate;
mod contract;
mod error;
mod ids;
mod merge_base;

mod records;
mod schema;
mod sql;
mod transfer;
mod wire;

pub use candidate::*;
pub use contract::{
    AdmissionSetReceipt, BranchCommit, BranchSource, DatabaseReceipt, HeadMoved, InventoryEntry,
    InventoryPage, LayerInitialization, LayerSource, LocalAdmissionReceipt, LocalObjectReceipt,
    MergeOutcome, ObjectTransferReceipt, ReadOnlyHistory, RefOutcome, StorageReceipt,
    StoreStorageSnapshot, TransferReceipt, TransferSetReceipt, TransportReceipt, WrongHistory,
};
pub use error::{Result, StorageError};
pub use ids::*;
pub use merge_base::*;

pub use records::*;
pub use schema::*;
pub use sql::fact_batches;
#[doc(hidden)]
pub use transfer::{take_storage_receipts, DeferredFactStore, DEFERRED_MEMORY_BYTES};

pub(crate) use contract::{
    note_receiver_authentication, note_traversal_authentication, AdmissionStats,
};

#[doc(hidden)]
pub use contract::{
    BaseSnapshot, EndpointReply, EndpointRequest, EndpointResponse, ObjectSource, StackAttestation,
    StackPush, StoreEndpoint, TransferExchange, TransferIntent, TransferOutcome, TransferTarget,
};
#[doc(hidden)]
pub use transfer::TransferPipeline;
#[doc(hidden)]
pub use wire::*;

#[doc(hidden)]
pub mod internal {
    #[cfg(feature = "test-instrumentation")]
    pub use crate::contract::{
        reset_transfer_authentication_counts, transfer_authentication_counts,
    };
    pub use crate::contract::{
        BaseSnapshot, EndpointReply, EndpointRequest, EndpointResponse, ObjectSource,
        StackAttestation, StackPush, StoreEndpoint, TransferExchange, TransferIntent,
        TransferOutcome, TransferTarget,
    };
    pub use crate::transfer::TransferPipeline;
    pub use crate::wire::*;
}

#[cfg(test)]
#[path = "../tests/support/sql_unit.rs"]
mod sql_tests;
