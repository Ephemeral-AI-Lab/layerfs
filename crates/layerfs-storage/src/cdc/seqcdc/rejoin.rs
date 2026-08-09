//! SeqCDC update/rejoin algorithm binding.

/// Frozen tag used to bind update/rejoin evidence to SeqCDC/OS.
pub(in crate::cdc) const ALGORITHM_TAG: [u8; 8] = *b"OSCDC001";
