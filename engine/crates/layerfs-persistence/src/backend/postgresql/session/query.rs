//! Engine-neutral parameters bound to a prepared statement's declared types.
use crate::backend::records::{BackendError, Param};
use postgres::types::{ToSql, Type};

pub(crate) type Bound = Box<dyn ToSql + Sync>;

/// Binds one parameter to the type the server inferred for it. Integers
/// narrow only when they fit; any other mismatch is refused before sending.
pub(crate) fn bind(param: Param, ty: &Type) -> Result<(Bound, u64), BackendError> {
    let narrow = |v: i64| -> Result<Bound, BackendError> {
        Ok(if *ty == Type::INT8 {
            Box::new(v)
        } else if *ty == Type::INT4 {
            Box::new(i32::try_from(v).map_err(|_| BackendError::Integrity)?)
        } else if *ty == Type::INT2 {
            Box::new(i16::try_from(v).map_err(|_| BackendError::Integrity)?)
        } else {
            return Err(BackendError::Integrity);
        })
    };
    Ok(match param {
        Param::I64(v) => (narrow(v)?, 8),
        Param::Text(v) => (v.len() as u64, v).into_bound(),
        Param::Bytes(v) => (v.len() as u64, v).into_bound(),
        Param::OptionalBytes(v) => (v.as_ref().map_or(0, Vec::len) as u64, v).into_bound(),
        Param::OptionalText(v) => (v.as_ref().map_or(0, String::len) as u64, v).into_bound(),
    })
}

trait IntoBound {
    fn into_bound(self) -> (Bound, u64);
}
impl<T: ToSql + Sync + 'static> IntoBound for (u64, T) {
    fn into_bound(self) -> (Bound, u64) {
        (Box::new(self.1), self.0)
    }
}
