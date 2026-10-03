//! Owned, bounded binding inputs for the one I/O worker.
use postgres::types::{ToSql, Type};
pub(crate) enum Param {
    Bytes(Vec<u8>),
    OptionalBytes(Option<Vec<u8>>),
    Text(String),
    Texts(Vec<String>),
    I64(i64),
    OptionalI64(Option<i64>),
    I64s(Vec<i64>),
    I16s(Vec<i16>),
    ByteArrays(Vec<Vec<u8>>),
    NullableByteArrays(Vec<Option<Vec<u8>>>),
}
impl Param {
    pub(crate) fn binding(&self) -> (&(dyn ToSql + Sync), Type) {
        match self {
            Self::Bytes(value) => (value, Type::BYTEA),
            Self::OptionalBytes(value) => (value, Type::BYTEA),
            Self::Text(value) => (value, Type::TEXT),
            Self::Texts(value) => (value, Type::TEXT_ARRAY),
            Self::I64(value) => (value, Type::INT8),
            Self::OptionalI64(value) => (value, Type::INT8),
            Self::I64s(value) => (value, Type::INT8_ARRAY),
            Self::I16s(value) => (value, Type::INT2_ARRAY),
            Self::ByteArrays(value) => (value, Type::BYTEA_ARRAY),
            Self::NullableByteArrays(value) => (value, Type::BYTEA_ARRAY),
        }
    }
}
