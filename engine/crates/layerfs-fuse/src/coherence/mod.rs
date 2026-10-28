//! Kernel-cache coherence rules. No rule here sends a kernel notification or
//! waits for an entry or attribute lifetime to expire.
pub mod attributes;
pub mod pages;
pub mod reply_order;
