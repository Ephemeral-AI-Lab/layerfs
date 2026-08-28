//! Projection timer values.

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ProjectionTimerAvailability {
    Available,
    #[default]
    Unavailable,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ProjectionTimer {
    pub availability: ProjectionTimerAvailability,
    pub nanoseconds: u64,
}

impl ProjectionTimer {
    pub const fn available() -> Self {
        Self {
            availability: ProjectionTimerAvailability::Available,
            nanoseconds: 0,
        }
    }

    pub(super) fn checked_delta(self, before: Self) -> Option<Self> {
        (self.availability == before.availability).then_some(Self {
            availability: self.availability,
            nanoseconds: self.nanoseconds.checked_sub(before.nanoseconds)?,
        })
    }

    pub(super) fn checked_add(self, other: Self) -> Option<Self> {
        if self.availability == ProjectionTimerAvailability::Available
            && other.availability == ProjectionTimerAvailability::Available
        {
            Some(Self {
                availability: ProjectionTimerAvailability::Available,
                nanoseconds: self.nanoseconds.checked_add(other.nanoseconds)?,
            })
        } else {
            Some(Self::default())
        }
    }
}
