//! Store-family integrity verification.

pub mod full;
mod policy;

pub(crate) use full::{
    closure::verify_root,
    history::{
        authenticated_closure_for_each, retained_union, verify_retained_union_observed_counted,
        RetainedUnion,
    },
    object::VerificationObservation,
};
pub use policy::IntegrityMode;
