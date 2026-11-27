//! Overwriting a secret before its memory is freed (#819 P-B): a password,
//! an address that holds one, a frame, request or record that carries one.
//!
//! Every byte of the buffer's allocation, its spare capacity included, is
//! set to zero and then handed to [`std::hint::black_box`] between
//! [`compiler_fence`]s. Control forbids `unsafe` code, so these are
//! ordinary writes, not `std::ptr::write_volatile`: `black_box` makes it
//! likely that the compiler keeps them, but Rust does not guarantee it. No
//! dependency is added for it. Copies a buffer left behind when it grew,
//! and copies made by code that is not LayerFS's, are not reached.
use layerfs_protocol::provision::install::SdkValue;
use std::sync::atomic::{compiler_fence, Ordering};

/// Overwrites `bytes`, spare capacity included, then frees them.
pub fn bytes(mut bytes: Vec<u8>) {
    // within its capacity a resize writes the spare bytes in place
    bytes.resize(bytes.capacity(), 0);
    bytes.fill(0);
    compiler_fence(Ordering::SeqCst);
    std::hint::black_box(&bytes);
    compiler_fence(Ordering::SeqCst);
}

/// Overwrites `text`, spare capacity included, then frees it.
pub fn text(text: String) {
    bytes(text.into_bytes());
}

/// Overwrites every text, name and byte string of `value`, then frees it.
pub fn value(value: SdkValue) {
    match value {
        SdkValue::Text(text) => self::text(text),
        SdkValue::Bytes(data) => bytes(data),
        SdkValue::List(items) => items.into_iter().for_each(self::value),
        SdkValue::Map(fields) => {
            for (name, field) in fields {
                self::text(name);
                self::value(field);
            }
        }
        SdkValue::Null | SdkValue::Bool(_) | SdkValue::Int(_) => {}
    }
}

/// A value overwritten ([`value`]) when it is dropped on any path, when
/// `armed`; an unarmed one is only freed.
pub struct Guard {
    value: Option<SdkValue>,
    armed: bool,
}

impl Guard {
    /// Guards `value`, overwritten on drop when `armed`.
    pub fn new(value: SdkValue, armed: bool) -> Self {
        Self {
            value: Some(value),
            armed,
        }
    }

    /// The guarded value.
    pub fn get(&self) -> &SdkValue {
        self.value.as_ref().unwrap_or(&SdkValue::Null)
    }

    /// The value, no longer guarded.
    pub fn take(mut self) -> SdkValue {
        self.value.take().unwrap_or_default()
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        if let Some(value) = self.value.take().filter(|_| self.armed) {
            self::value(value);
        }
    }
}
