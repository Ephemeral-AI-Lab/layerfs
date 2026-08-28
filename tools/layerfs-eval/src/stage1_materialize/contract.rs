pub(in crate::stage1_materialize) const FILE_PATH: &str = "data/payload.bin";
pub(in crate::stage1_materialize) const BUFFER_BYTES: usize = 1024 * 1024;
pub(in crate::stage1_materialize) const FIXTURE_MODE: u32 = 0o644;
pub(in crate::stage1_materialize) const FIXTURE_MTIME_SECONDS: u64 = 1_700_000_123;
pub(in crate::stage1_materialize) const FIXTURE_MTIME_NANOSECONDS: u32 = 456_789_123;
pub(in crate::stage1_materialize) const PRESERVED_24_MIB_DIGEST: &str =
    "89dcf8d2f5ce72728b9ef7c9e955de6299738140f35686015ec9bfef5f598ca5";

pub(in crate::stage1_materialize) type EvalResult<T> = Result<T, String>;
