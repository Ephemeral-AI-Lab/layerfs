//! Finite producer custody. Queued/in-flight records retain their producer slot.
use super::Output;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
struct Registry {
    slots: Vec<AtomicBool>,
}
pub(crate) struct Registration {
    registry: Arc<Registry>,
    index: usize,
}
impl Drop for Registration {
    fn drop(&mut self) {
        self.registry.slots[self.index].store(false, Ordering::Release);
    }
}
/// Bounded host-side producer admission sharing an explicitly configured output.
pub struct Collector {
    registry: Arc<Registry>,
    output: Output,
}
/// Connected/closing ownership; drop cannot release a slot still used by output.
pub struct Producer {
    registration: Arc<Registration>,
    output: Output,
}
impl Collector {
    /// Validates the 1..=16 aggregate producer ceiling before allocating slots.
    pub fn new(producers: usize, output: Output) -> Option<Self> {
        if producers == 0 || producers > 16 {
            return None;
        }
        Some(Self {
            registry: Arc::new(Registry {
                slots: (0..producers).map(|_| AtomicBool::new(false)).collect(),
            }),
            output,
        })
    }
    /// Immediately admits or refuses one producer, including closing resources.
    pub fn connect(&self) -> Option<Producer> {
        for (index, slot) in self.registry.slots.iter().enumerate() {
            if slot
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return Some(Producer {
                    registration: Arc::new(Registration {
                        registry: self.registry.clone(),
                        index,
                    }),
                    output: self.output.clone(),
                });
            }
        }
        None
    }
    /// Connected plus closing producers; historical incarnations are not retained.
    pub fn occupied(&self) -> usize {
        self.registry
            .slots
            .iter()
            .filter(|s| s.load(Ordering::Acquire))
            .count()
    }
}
impl Producer {
    /// Accepts one complete bounded envelope from a separately bounded stream
    /// decoder. It is retained as opaque bytes, never expanded into an imported tree.
    /// JSON validity/identity are the producer codec and coordinator's responsibility.
    pub fn submit(&self, bytes: Vec<u8>) -> bool {
        if bytes.len() > 16384 || !bytes.starts_with(b"LFT1 {") || !bytes.ends_with(b"}\n") {
            return false;
        }
        self.output
            .submit_registered(bytes, Some(self.registration.clone()));
        true
    }
}
