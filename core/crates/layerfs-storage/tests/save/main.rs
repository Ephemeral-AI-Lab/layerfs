mod captured_payload_retention;
#[path = "../support/chain_provider.rs"]
mod chains;
mod concurrent_winner_chain;
mod demand_locality;
mod encoder_reuse;
mod exact_reuse_trust;
mod kept_ids_pooled_chains;
#[path = "../support/memory_metadata.rs"]
mod metadata;
mod resolved_reads;
mod ring_prefix_admission;
#[path = "../support/mod.rs"]
mod support;
mod wave_reference_locates;
