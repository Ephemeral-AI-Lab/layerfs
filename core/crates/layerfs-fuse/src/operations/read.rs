//! Bounded READ/READLINK consumers over original completed local windows.
use super::lookup::Custody;
use super::{NativeRead, ReadFailure};
use crate::ports::ServiceError;
use layerfs_overlay::{FileRead, OverlayError, CELL_BYTES, READ_WINDOW};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadDataInput {
    File { offset: u64, length: u32 },
    Link,
}

pub struct NativeData {
    data: Vec<u8>,
    custody: Custody,
}
impl NativeData {
    pub fn bytes(&self) -> &[u8] {
        &self.data
    }
    /// Reply data is gone before either processing reference can be released.
    pub async fn dispose(self) -> Result<(), ReadFailure> {
        let Self { data, custody } = self;
        drop(data);
        custody.dispose().await
    }
}
impl NativeRead {
    pub async fn read_file(self, offset: u64, length: u32) -> Result<NativeData, ReadFailure> {
        self.data(offset, length, false).await
    }
    pub async fn readlink(self) -> Result<NativeData, ReadFailure> {
        self.data(0, CELL_BYTES as u32, true).await
    }
    async fn data(self, offset: u64, length: u32, link: bool) -> Result<NativeData, ReadFailure> {
        let read = match self.value() {
            Ok(value) if value.file.is_none() && value.directory.is_none() => value.read,
            _ => return Err(self.retain(Box::new(OverlayError::Invalid("native data answer")))),
        };
        let Self { value, mut custody } = self;
        custody.data_input = Some(if link {
            ReadDataInput::Link
        } else {
            ReadDataInput::File { offset, length }
        });
        // Data requests have consumed their metadata answer. Its original Arc
        // and every projection end before the next ordinary owner job; keeping
        // all16 such completions would exhaust the16 ordinary job slots. The
        // independent FileRead/source capabilities continue to protect bytes.
        drop(value);
        custody.plan = None;
        custody.receipt = None;
        let result = custody.window(read, offset, length, link).await;
        match result {
            Ok(data) => Ok(NativeData { data, custody }),
            Err(reason) => Err(ReadFailure::new(reason, custody)),
        }
    }
}
impl Custody {
    async fn window(
        &self,
        read: FileRead,
        offset: u64,
        length: u32,
        link: bool,
    ) -> Result<Vec<u8>, ServiceError> {
        if length as usize > READ_WINDOW {
            return Err(Box::new(OverlayError::Invalid("native read window")));
        }
        let local = self.services.local_read(read, offset, length).await?;
        let immutable = self.services.immutable(self.view.as_ref().unwrap()).await?;
        let data = if link {
            immutable
                .readlink_window(read, local.get().clone())
                .map(|target| target.as_bytes().to_vec())
        } else {
            let mut data = Vec::with_capacity(length as usize);
            immutable
                .read_file_window(read, offset, length, local.get().clone(), &mut data)
                .map(|_| data)
        };
        drop(immutable);
        drop(local);
        data.map_err(|error| self.services.failed_base_read(error.into()))
    }
}
