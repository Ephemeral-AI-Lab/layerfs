//! Owned bounded scalar inputs/results, independent of database engines.
#[derive(Clone, Debug)]
pub(crate) enum Param {
    I64(i64),
    Text(String),
    Bytes(Vec<u8>),
    OptionalBytes(Option<Vec<u8>>),
}
#[derive(Clone, Debug)]
pub(crate) enum Value {
    Null,
    Integer(i64),
    Text(String),
    Bytes(Vec<u8>),
}
pub(crate) struct Record {
    pub(crate) cells: Vec<Value>,
}
pub(crate) trait FromValue: Sized {
    fn from_value(value: &Value) -> Option<Self>;
}
impl FromValue for i64 {
    fn from_value(v: &Value) -> Option<Self> {
        if let Value::Integer(v) = v {
            Some(*v)
        } else {
            None
        }
    }
}
impl FromValue for Vec<u8> {
    fn from_value(v: &Value) -> Option<Self> {
        if let Value::Bytes(v) = v {
            Some(v.clone())
        } else {
            None
        }
    }
}
impl FromValue for String {
    fn from_value(v: &Value) -> Option<Self> {
        if let Value::Text(v) = v {
            Some(v.clone())
        } else {
            None
        }
    }
}
impl<T: FromValue> FromValue for Option<T> {
    fn from_value(v: &Value) -> Option<Self> {
        match v {
            Value::Null => Some(None),
            _ => T::from_value(v).map(Some),
        }
    }
}
impl Record {
    pub(crate) fn take_bytes(&mut self, index: usize) -> Result<Vec<u8>, BackendError> {
        match self
            .cells
            .get_mut(index)
            .map(|v| std::mem::replace(v, Value::Null))
        {
            Some(Value::Bytes(v)) => Ok(v),
            _ => Err(BackendError::Integrity),
        }
    }
    pub(crate) fn get<T: FromValue>(&self, index: usize) -> Result<T, BackendError> {
        self.cells
            .get(index)
            .and_then(T::from_value)
            .ok_or(BackendError::Integrity)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BackendError {
    Busy,
    ReadOnly,
    Capacity,
    Integrity,
    Filesystem(std::io::ErrorKind),
    Unknown,
}
impl From<BackendError> for layerfs_storage::port::PersistenceError {
    fn from(e: BackendError) -> Self {
        match e {
            BackendError::Unknown => Self::Uncertain,
            BackendError::Integrity => Self::Malformed,
            _ => Self::Refused {
                status: format!("{e:?}"),
            },
        }
    }
}
impl From<BackendError> for layerfs_history::HistoryError {
    fn from(e: BackendError) -> Self {
        match e {
            BackendError::Busy => Self::Busy,
            BackendError::ReadOnly => Self::ContinuityUnavailable,
            BackendError::Capacity => Self::Capacity("persistence"),
            BackendError::Integrity => Self::Integrity("persistence"),
            BackendError::Filesystem(_) => Self::ContinuityUnavailable,
            BackendError::Unknown => Self::UnknownOutcome,
        }
    }
}

/// Recognizes uncertain outcomes without inspecting error text or guessing rollback.
pub(crate) trait OutcomeError {
    fn uncertain(&self) -> bool;
}
impl OutcomeError for BackendError {
    fn uncertain(&self) -> bool {
        *self == Self::Unknown
    }
}
impl OutcomeError for layerfs_storage::port::PersistenceError {
    fn uncertain(&self) -> bool {
        matches!(self, Self::Uncertain)
    }
}
impl OutcomeError for layerfs_history::HistoryError {
    fn uncertain(&self) -> bool {
        self.unknown()
    }
}
