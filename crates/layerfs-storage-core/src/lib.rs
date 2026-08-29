#![forbid(unsafe_code)]

mod admission;
mod contract;
mod ids;
mod merge_base;
mod merkle;
mod records;
mod schema;
mod sql;
mod three_way;
mod wire;

pub use contract::{
    AddLayerSource, Change, Conflict, HeadMoved, MergeOutcome, ReadOnlyHistory, RefOutcome, Result,
    StorageError, WrongHistory,
};
pub use ids::*;
pub use merge_base::*;
pub use merkle::*;
pub use records::*;
pub use schema::*;
pub use sql::fact_batches;
pub use three_way::*;

pub(crate) use contract::{
    note_receiver_authentication, note_traversal_authentication, AdmissionStats, TransferStats,
};

#[doc(hidden)]
pub use admission::TransferPipeline;
#[doc(hidden)]
pub use contract::{
    BaseSnapshot, EndpointReply, EndpointRequest, EndpointResponse, ObjectSource, StackAttestation,
    StackPush, StagedChange, StoreEndpoint, TransferExchange, TransferIntent, TransferOutcome,
    TransferTarget,
};
#[doc(hidden)]
pub use wire::*;

#[doc(hidden)]
pub mod internal {
    pub use crate::admission::TransferPipeline;
    #[cfg(feature = "test-instrumentation")]
    pub use crate::contract::{
        reset_transfer_authentication_counts, transfer_authentication_counts,
    };
    pub use crate::contract::{
        BaseSnapshot, EndpointReply, EndpointRequest, EndpointResponse, ObjectSource,
        StackAttestation, StackPush, StagedChange, StoreEndpoint, TransferExchange, TransferIntent,
        TransferOutcome, TransferTarget,
    };
    pub use crate::wire::*;
}
