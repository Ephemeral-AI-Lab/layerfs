#[cfg(feature = "local_folder")]
pub mod local_folder;
#[cfg(feature = "s3_api")]
pub mod r2;
#[cfg(feature = "minio")]
pub mod s3;
