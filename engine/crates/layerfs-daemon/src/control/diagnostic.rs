//! Bounded, nonblocking registry observations, without Store or overlay reads.
use super::{inspect::activity, registry::Entry, Service};
use layerfs_bridge::control::{Reply, SdkCall, SdkRefusal, SdkValue};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

const PAGE: usize = 32;
fn refused(code: &str, message: &str) -> Reply {
    Reply::SdkRefused(SdkRefusal {
        code: code.into(),
        message: message.into(),
        detail: SdkValue::Null,
    })
}
impl Service {
    /// One diagnostic page of registry facts; no history, Store or overlay call.
    pub(crate) fn diagnostic_workspaces(&self, call: &SdkCall) -> Reply {
        let offset = match call.args.get_int("offset").unwrap_or(0) {
            value if value >= 0 => match usize::try_from(value) {
                Ok(value) => value,
                Err(_) => return refused("InvalidInput", "diagnostic offset"),
            },
            _ => return refused("InvalidInput", "diagnostic offset"),
        };
        let entries = match self.entries.try_lock() {
            Ok(entries) => entries,
            Err(std::sync::TryLockError::WouldBlock) => {
                return refused("Busy", "control registry admission busy")
            }
            Err(_) => return refused("Failed", "control registry poisoned"),
        };
        let mut hash = DefaultHasher::new();
        for (id, entry) in entries.iter() {
            id.to_bytes().hash(&mut hash);
            match entry {
                Entry::Binding => 0u64.hash(&mut hash),
                Entry::Bound(binding) => binding.epoch.hash(&mut hash),
            }
        }
        let generation = format!("{:016x}", hash.finish());
        if call
            .args
            .get_text("generation")
            .is_some_and(|expected| expected != generation)
        {
            return refused(
                "Changed",
                "workspace registry changed during diagnostic pagination",
            );
        }
        if offset > entries.len() {
            return refused("InvalidInput", "diagnostic offset beyond registry");
        }
        let rows = entries
            .iter()
            .skip(offset)
            .take(PAGE)
            .map(|(id, entry)| {
                let (state, mounted) = match entry {
                    Entry::Binding => ("binding", false),
                    Entry::Bound(binding) => (
                        activity(binding.activity),
                        binding.kernel.block().phase == layerfs_bridge::control::MountPhase::Ready,
                    ),
                };
                SdkValue::map([
                    (
                        "workspace",
                        SdkValue::text(layerfs_store_service::history::lookup::hex(&id.to_bytes())),
                    ),
                    ("activity", SdkValue::text(state)),
                    ("mounted", SdkValue::Bool(mounted)),
                ])
            })
            .collect::<Vec<_>>();
        let next = offset + rows.len();
        Reply::Sdk(SdkValue::map([
            ("generation", SdkValue::text(generation)),
            ("total", SdkValue::Int(entries.len() as i64)),
            (
                "next_offset",
                if next < entries.len() {
                    SdkValue::Int(next as i64)
                } else {
                    SdkValue::Null
                },
            ),
            ("workspaces", SdkValue::List(rows)),
        ]))
    }
}
