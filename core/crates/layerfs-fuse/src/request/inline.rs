//! Declared inline answers and bounded no-reply ownership units.
use super::{
    accounting::{Disposal, Opcode},
    failure::{failed, KernelInput},
    state::NativeFilesystem,
};
use crate::RequestDisposition;
use fuser::{INodeNo, ReplyStatfs};
use std::{cell::Cell, io};

/// Fixed declared values. They report no physical capacity and are never an
/// admission signal; the all-zero library default is not used.
const BLOCK_BYTES: u32 = 4096;
const NAME_BYTES: u32 = 255;
const DECLARED_BLOCKS: u64 = 1 << 30;
const DECLARED_FILES: u64 = 1 << 30;

pub(super) fn statfs(reply: ReplyStatfs) {
    reply.statfs(
        DECLARED_BLOCKS,
        DECLARED_BLOCKS,
        DECLARED_BLOCKS,
        DECLARED_FILES,
        DECLARED_FILES,
        BLOCK_BYTES,
        NAME_BYTES,
        BLOCK_BYTES,
    );
}

thread_local! {
    /// Last ownership frame seen by this receive loop, and whether it has
    /// already been counted as carrying several units.
    static FRAME: Cell<(u64, bool)> = const { Cell::new((0, false)) };
}

impl NativeFilesystem {
    /// The pinned library delivers a batch as repeated single callbacks sharing
    /// one request identity and does not expose the opcode. `Forget` therefore
    /// counts ownership frames and `BatchForget` those observed with a second
    /// unit; every unit is admitted, charged and disposed independently.
    pub(super) fn forget_unit(&self, request: u64, inode: INodeNo, count: u64) {
        let (last, batched) = FRAME.get();
        if request == 0 || request != last {
            self.accounting.opcode(Opcode::Forget);
            FRAME.set((request, false));
        } else if !batched {
            self.accounting.opcode(Opcode::BatchForget);
            FRAME.set((request, true));
        }
        self.accounting.forget_unit();
        let permit = match self.queue.receive().map(|received| received.admit(0)) {
            Ok(Ok(permit)) => permit,
            // Terminal admission: the kernel reference stays in indexed custody
            // until drain-qualified revocation. No decrement is guessed.
            Ok(Err(_)) | Err(_) => return self.accounting.disposed(Disposal::Unadmitted),
        };
        let mount = self.queue.identity();
        let serial = self.identity.serial(inode);
        let services = self.services.clone();
        self.handoff(
            permit,
            Box::pin(async move {
                let outcome = async {
                    let serial =
                        serial.map_err(|error| io::Error::from_raw_os_error(error.code()))?;
                    let services = services.request()?;
                    drop(services.forget(mount, serial, count).await?);
                    Ok::<_, crate::ports::ServiceError>(())
                }
                .await;
                match outcome {
                    Ok(()) => RequestDisposition::Complete,
                    Err(error) => failed(
                        request,
                        mount,
                        KernelInput::Forget {
                            inode: inode.0,
                            count,
                        },
                        error,
                    ),
                }
            }),
        );
    }
}
