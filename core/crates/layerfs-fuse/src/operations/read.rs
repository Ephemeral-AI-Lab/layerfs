//! READ and READLINK: one owner visit, at most one base read, then the reply.
use super::lookup::Custody;
use super::ReadFailure;
use crate::ports::{RequestServices, ServiceError, ServiceReply};
use layerfs_overlay::{NativeMount, OverlayError, CELL_BYTES, READ_WINDOW};
use layerfs_workspace::{NativeReadOperation, NativeWindow, Refusal};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadDataInput {
    File { offset: u64, length: u32 },
    Link,
}

/// The bytes of one READ or READLINK reply. Its request holds nothing in the
/// engine: no source, reader or lease was recorded, so nothing is released
/// after the reply and dropping this value is all that ends it.
pub struct NativeData(Bytes);
enum Bytes {
    /// A file window the visit decided whole, still in its completion.
    Visit(ServiceReply<Arc<NativeWindow>>),
    Owned(Vec<u8>),
}
impl NativeData {
    pub fn bytes(&self) -> &[u8] {
        match &self.0 {
            Bytes::Visit(window) => window.get().whole(false).unwrap_or_default(),
            Bytes::Owned(data) => data,
        }
    }
    /// One read-only owner visit copies the window's local bytes and names
    /// the base root the rest belongs to. Only when the rest is needed does
    /// the request leave the receive loop, take one Store reader and read it
    /// at that root. A window decided locally, a refusal and a window at or
    /// beyond a remembered end of file take no reader and no hand-off.
    ///
    /// The answer is the inode as it stood at the visit. A failure owns
    /// nothing in the engine either.
    pub async fn read(
        services: Arc<dyn RequestServices>,
        mount: NativeMount,
        request: u64,
        serial: u64,
        handle: Option<u64>,
        input: ReadDataInput,
    ) -> Result<Result<Self, Refusal>, ReadFailure> {
        let operation = NativeReadOperation::Data { serial };
        let mut custody = Custody::unowned(services, mount, request, serial, handle, operation);
        custody.data_input = Some(input);
        match custody.window(input).await {
            Ok(answer) => Ok(answer),
            Err(reason) => Err(ReadFailure::new(reason, custody)),
        }
    }
}
impl Custody {
    async fn window(
        &self,
        input: ReadDataInput,
    ) -> Result<Result<NativeData, Refusal>, ServiceError> {
        let (offset, length, link) = match input {
            ReadDataInput::File { offset, length } => (offset, length, false),
            ReadDataInput::Link => (0, CELL_BYTES as u32, true),
        };
        if length as usize > READ_WINDOW {
            return Err(Box::new(OverlayError::Invalid("native read window")));
        }
        let (mount, serial, handle) = self.target();
        let visit = self
            .services
            .read_visit(mount, serial, handle, offset, length)
            .await?;
        if visit.get().whole(link).is_some() {
            return Ok(Ok(NativeData(Bytes::Visit(visit))));
        }
        // The request's own copy: the job and its credit are gone before any
        // provider wait.
        let window = NativeWindow::clone(visit.get());
        drop(visit);
        let base = if window.inherits(offset, link) {
            // Provider reads never run on the loop that received the request.
            crate::LeaveReceiver::default().await;
            Some(self.services.base().await?)
        } else {
            None
        };
        let answer = window.finish(base.as_ref(), serial, offset, length, link);
        drop(base); // Return the actual reader before the reply.
        match answer {
            Ok(answer) => Ok(answer.map(|data| NativeData(Bytes::Owned(data)))),
            Err(error) => Err(self.services.failed_base_read(error.into())),
        }
    }
}
