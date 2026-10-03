//! Owned scalar bindings for history statements.
use crate::backend::records::Param;
pub(crate) trait Parameters {
    fn into_params(self) -> Vec<Param>;
}
impl Parameters for Vec<Param> {
    fn into_params(self) -> Vec<Param> {
        self
    }
}
impl<T: Into<Param>, const N: usize> Parameters for [T; N] {
    fn into_params(self) -> Vec<Param> {
        self.into_iter().map(Into::into).collect()
    }
}
macro_rules! params { ($($value:expr),* $(,)?) => { vec![$(crate::backend::records::Param::from($value)),*] }; }
pub(crate) use params;
impl From<&[u8]> for Param {
    fn from(v: &[u8]) -> Self {
        Self::Bytes(v.to_vec())
    }
}
impl From<Vec<u8>> for Param {
    fn from(v: Vec<u8>) -> Self {
        Self::Bytes(v)
    }
}
impl From<Option<Vec<u8>>> for Param {
    fn from(v: Option<Vec<u8>>) -> Self {
        Self::OptionalBytes(v)
    }
}
impl From<&str> for Param {
    fn from(v: &str) -> Self {
        Self::Text(v.to_owned())
    }
}
impl From<String> for Param {
    fn from(v: String) -> Self {
        Self::Text(v)
    }
}
impl From<i64> for Param {
    fn from(v: i64) -> Self {
        Self::I64(v)
    }
}
