//! Bounded native classes retain handshake, active and terminal owners until join.
use layerfs_bridge::{
    adapters::native::purpose::{Hello, Purpose},
    contract::{Code, Failure},
};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Class {
    Pending,
    General,
    Catalog,
    Control,
}
struct Ledger {
    live: [usize; 4],
    general: usize,
    protected: bool,
    pending: usize,
}
pub(super) struct PurposeAdmission {
    ledger: Arc<Mutex<Ledger>>,
    slots: usize,
}
pub(super) struct SessionSlot {
    ledger: Arc<Mutex<Ledger>>,
    class: Mutex<Class>,
}
impl PurposeAdmission {
    pub(super) fn new(general: usize, protected: bool) -> Result<Self, Failure> {
        if general == 0 {
            return Err(Code::InvalidInput.into());
        }
        if protected && general > 4 {
            return Err(Code::Unsupported.into());
        }
        let slots = general
            .checked_add(if protected { 3 } else { 0 })
            .ok_or(Code::Capacity)?;
        Ok(Self {
            ledger: Arc::new(Mutex::new(Ledger {
                live: [0; 4],
                general,
                protected,
                pending: if protected { 1 } else { general },
            })),
            slots,
        })
    }
    pub(super) fn slots(&self) -> usize {
        self.slots
    }
    pub(super) fn pending(&self) -> Result<Arc<SessionSlot>, Failure> {
        let mut ledger = self.ledger.lock().map_err(|_| Code::Ownership)?;
        if ledger.live[0] >= ledger.pending {
            return Err(Code::Capacity.into());
        }
        let slot = Arc::new(SessionSlot {
            ledger: Arc::clone(&self.ledger),
            class: Mutex::new(Class::Pending),
        });
        ledger.live[0] += 1;
        Ok(slot)
    }
}
impl SessionSlot {
    pub(super) fn select(&self, hello: Hello) -> Result<(), Failure> {
        let mut class = self.class.lock().map_err(|_| Code::Ownership)?;
        if *class != Class::Pending {
            return Err(Code::Ownership.into());
        }
        let mut ledger = self.ledger.lock().map_err(|_| Code::Ownership)?;
        let (next, at, capacity) = match hello.purpose() {
            Purpose::General => (Class::General, 1, ledger.general),
            Purpose::Catalog if ledger.protected && hello.version() == 2 => (Class::Catalog, 2, 1),
            Purpose::Control if ledger.protected && hello.version() == 2 => (Class::Control, 3, 1),
            _ => return Err(Code::Unsupported.into()),
        };
        if ledger.live[at] >= capacity {
            return Err(Code::Capacity.into());
        }
        if ledger.live[0] == 0 {
            return Err(Code::Ownership.into());
        }
        ledger.live[at] += 1;
        ledger.live[0] -= 1;
        *class = next;
        Ok(())
    }
}
impl Drop for SessionSlot {
    fn drop(&mut self) {
        // The acceptor holds the last Arc until it joined the worker and closed
        // the shutdown descriptor. Failure never transfers this slot to idle.
        if let (Ok(class), Ok(mut ledger)) = (self.class.lock(), self.ledger.lock()) {
            let at = match *class {
                Class::Pending => 0,
                Class::General => 1,
                Class::Catalog => 2,
                Class::Control => 3,
            };
            if ledger.live[at] > 0 {
                ledger.live[at] -= 1;
            }
        }
    }
}
