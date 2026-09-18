//! One clock domain, and the bracket bookkeeping that proves a phase was timed.
//!
//! **Two clocks, not three.** `CLOCK_MONOTONIC_RAW` (id 4) is the only clock in
//! this harness, in Rust and in Python alike. `std::time::Instant` is deliberately
//! *not* used for a recorded figure: on this platform it is a different domain, and
//! two domains in one receipt is how a phase gets attributed to the wrong window.
//!
//! **What this module does not claim.** `Sigma self_ns == root.elapsed_ns` is a
//! tautology of the product's own tree arithmetic, not an `attach` detector, and
//! nothing here presents it as one. What is checked instead is the thing that can
//! actually fail: every child bracket lies inside its parent's bracket, siblings do
//! not overlap, and the root bracket covers every recorded window.

/// `CLOCK_MONOTONIC_RAW` on Darwin and Linux.
pub const CLOCK_MONOTONIC_RAW: i32 = 4;

/// A C `timespec`.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Timespec {
    tv_sec: i64,
    tv_nsec: i64,
}

#[cfg(unix)]
unsafe extern "C" {
    fn clock_gettime(clock_id: i32, tp: *mut Timespec) -> i32;
}

/// Reads `CLOCK_MONOTONIC_RAW` in nanoseconds.
///
/// Returns `None` rather than a fabricated zero when the clock is unavailable: a
/// phase with no clock reading is `INCOMPLETE`, never `0 ns`.
pub fn mono_raw_ns() -> Option<u64> {
    #[cfg(unix)]
    {
        let mut now = Timespec::default();
        // SAFETY: clock_gettime writes exactly one C-layout timespec.
        let status = unsafe { clock_gettime(CLOCK_MONOTONIC_RAW, std::ptr::from_mut(&mut now)) };
        if status != 0 {
            return None;
        }
        let seconds = u64::try_from(now.tv_sec).ok()?;
        let nanos = u64::try_from(now.tv_nsec).ok()?;
        seconds.checked_mul(1_000_000_000)?.checked_add(nanos)
    }
    #[cfg(not(unix))]
    {
        None
    }
}

/// One recorded bracket: a label, its parent, and the clock stamps around it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Bracket {
    /// Index into [`WindowTree::brackets`].
    pub id: u32,
    /// Enclosing bracket, `None` for the root.
    pub parent: Option<u32>,
    /// Phase label as recorded in the trace.
    pub label: String,
    /// Clock reading when the bracket opened.
    pub open_ns: u64,
    /// Clock reading when the bracket closed.
    pub close_ns: u64,
}

/// Why a set of brackets is not a valid envelope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Defect {
    /// `close_ns < open_ns`.
    Unbalanced {
        /// Offending bracket.
        id: u32,
    },
    /// A child extends past its parent.
    NotContained {
        /// Offending child.
        id: u32,
        /// Its parent.
        parent: u32,
    },
    /// Two siblings overlap in time.
    SiblingOverlap {
        /// First sibling.
        left: u32,
        /// Second sibling.
        right: u32,
    },
    /// The root does not enclose every bracket.
    RootDoesNotEnclose {
        /// Bracket outside the root.
        id: u32,
    },
    /// A bracket names a parent that is not in the tree.
    UnknownParent {
        /// Offending bracket.
        id: u32,
        /// The parent it named.
        parent: u32,
    },
}

/// Reconstructed envelope over the harness's own brackets.
#[derive(Clone, Debug, Default)]
pub struct WindowTree {
    /// Brackets in record order; index 0 is the root by construction.
    pub brackets: Vec<Bracket>,
}

impl WindowTree {
    /// Opens the root bracket.
    pub fn open(label: &str) -> (Self, u64) {
        let now = mono_raw_ns().unwrap_or(0);
        let tree = Self {
            brackets: vec![Bracket {
                id: 0,
                parent: None,
                label: label.to_string(),
                open_ns: now,
                close_ns: now,
            }],
        };
        (tree, now)
    }

    /// Opens a child bracket and returns its id and open stamp.
    pub fn open_child(&mut self, parent: u32, label: &str) -> (u32, u64) {
        let now = mono_raw_ns().unwrap_or(0);
        let id = self.brackets.len() as u32;
        self.brackets.push(Bracket {
            id,
            parent: Some(parent),
            label: label.to_string(),
            open_ns: now,
            close_ns: now,
        });
        (id, now)
    }

    /// Closes a bracket. A second close overwrites, which `check` then reports.
    pub fn close(&mut self, id: u32) {
        let now = mono_raw_ns().unwrap_or(0);
        if let Some(bracket) = self.brackets.get_mut(id as usize) {
            bracket.close_ns = now;
        }
    }

    /// Root elapsed nanoseconds, from the one clock domain.
    pub fn root_elapsed_ns(&self) -> u64 {
        self.brackets
            .first()
            .map(|root| root.close_ns.saturating_sub(root.open_ns))
            .unwrap_or(0)
    }

    /// Every way this tree fails to be an envelope. Empty means it is one.
    pub fn check(&self) -> Vec<Defect> {
        let mut defects = Vec::new();
        let Some(root) = self.brackets.first() else {
            return defects;
        };
        for bracket in &self.brackets {
            if bracket.close_ns < bracket.open_ns {
                defects.push(Defect::Unbalanced { id: bracket.id });
            }
            match bracket.parent {
                None => {}
                Some(parent) => {
                    let Some(enclosing) = self.brackets.get(parent as usize) else {
                        defects.push(Defect::UnknownParent {
                            id: bracket.id,
                            parent,
                        });
                        continue;
                    };
                    if bracket.open_ns < enclosing.open_ns || bracket.close_ns > enclosing.close_ns {
                        defects.push(Defect::NotContained {
                            id: bracket.id,
                            parent,
                        });
                    }
                }
            }
            if bracket.id != 0
                && (bracket.open_ns < root.open_ns || bracket.close_ns > root.close_ns)
            {
                defects.push(Defect::RootDoesNotEnclose { id: bracket.id });
            }
        }
        let mut by_parent: Vec<(u32, Vec<&Bracket>)> = Vec::new();
        for bracket in &self.brackets {
            if let Some(parent) = bracket.parent {
                match by_parent.iter_mut().find(|(id, _)| *id == parent) {
                    Some((_, siblings)) => siblings.push(bracket),
                    None => by_parent.push((parent, vec![bracket])),
                }
            }
        }
        for (_, siblings) in by_parent {
            for (index, left) in siblings.iter().enumerate() {
                for right in siblings.iter().skip(index + 1) {
                    if left.open_ns < right.close_ns && right.open_ns < left.close_ns {
                        defects.push(Defect::SiblingOverlap {
                            left: left.id,
                            right: right.id,
                        });
                    }
                }
            }
        }
        defects
    }
}
