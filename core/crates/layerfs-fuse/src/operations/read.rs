//! Bounded READ/READLINK consumers over original completed local windows.
use super::{NativeRead, ReadFailure};
use crate::ports::ServiceError;
use layerfs_overlay::{OverlayError, CELL_BYTES, READ_WINDOW};

pub struct NativeData {
    data: Vec<u8>,
    request: NativeRead,
}
impl NativeData {
    pub fn bytes(&self) -> &[u8] {
        &self.data
    }
    /// Reply data is gone before either processing reference can be released.
    pub async fn dispose(self) -> Result<(), ReadFailure> {
        let Self { data, request } = self;
        drop(data);
        request.dispose().await
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
        let result = self.window(offset, length, link).await;
        match result {
            Ok(data) => Ok(NativeData {
                data,
                request: self,
            }),
            Err(reason) => {
                let Self { value, custody } = self;
                drop(value);
                Err(ReadFailure::new(reason, custody))
            }
        }
    }
    async fn window(&self, offset: u64, length: u32, link: bool) -> Result<Vec<u8>, ServiceError> {
        if length as usize > READ_WINDOW {
            return Err(Box::new(OverlayError::Invalid("native read window")));
        }
        let value = self
            .value()
            .map_err(|_| OverlayError::Invalid("native refused read"))?;
        let local = self
            .custody
            .services
            .local_read(value.read, offset, length)
            .await?;
        let immutable = self
            .custody
            .services
            .immutable(self.custody.view.as_ref().unwrap())
            .await?;
        let data = if link {
            immutable
                .readlink_window(value.read, local.get().clone())?
                .as_bytes()
                .to_vec()
        } else {
            let mut data = Vec::with_capacity(length as usize);
            immutable.read_file_window(
                value.read,
                offset,
                length,
                local.get().clone(),
                &mut data,
            )?;
            data
        };
        drop(immutable);
        drop(local);
        Ok(data)
    }
}
