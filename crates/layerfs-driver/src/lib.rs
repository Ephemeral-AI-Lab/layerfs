//! Private LayerFS projection and capture drivers.
//!
//! The first concrete driver will be Linux OverlayFS. This crate must not
//! implement identity, CDC, object admission, authority mutation, or its own
//! storage truth; it delegates those concerns to `layerfs-storage`.

#![forbid(unsafe_code)]
