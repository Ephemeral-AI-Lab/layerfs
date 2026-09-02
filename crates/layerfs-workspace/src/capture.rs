use crate::cow_tree::{Data, FileData, NodeId, Workspace};
use layerfs_content::file::rope::{self, FileStateRoot, RopeCounters};
use layerfs_layerstack_store::{DeferredObjectStore, ObjectBuffer, Result};
use std::io::{Cursor, Read};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};

#[derive(Default)]
pub(crate) enum CaptureState {
    #[default]
    Idle,
    Running {
        node: NodeId,
        next_offset: u64,
        sender: SyncSender<CaptureMessage>,
        thread: std::thread::JoinHandle<Result<CapturedContent>>,
    },
    Ready(Box<CapturedFile>),
    Invalid,
}

pub(crate) struct CapturedFile {
    pub(crate) node: NodeId,
    pub(crate) len: u64,
    pub(crate) root: FileStateRoot,
    pub(crate) counters: RopeCounters,
    pub(crate) objects: DeferredObjectStore,
}

pub(crate) struct CapturedContent {
    root: FileStateRoot,
    counters: RopeCounters,
    objects: DeferredObjectStore,
}

pub(crate) enum CaptureMessage {
    Bytes(Vec<u8>),
    Finish,
    Abort,
}

struct CaptureReader {
    receiver: Receiver<CaptureMessage>,
    current: Cursor<Vec<u8>>,
    done: bool,
}

impl Workspace {
    pub(crate) fn capture_write(&mut self, node: NodeId, offset: u64, old_len: u64, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let eligible = old_len == 0
            && offset == 0
            && matches!(
                self.nodes.get(&node).map(|node| &node.data),
                Some(Data::File(FileData::Overlay { base: None, .. }))
            );
        if matches!(self.capture, CaptureState::Idle)
            && (!eligible || self.start_capture(node).is_err())
        {
            self.capture = CaptureState::Invalid;
            return;
        }
        let sequential = matches!(
            &self.capture,
            CaptureState::Running {
                node: active,
                next_offset,
                ..
            } if *active == node && *next_offset == offset && old_len == offset
        );
        if !sequential {
            self.invalidate_capture();
            return;
        }
        let CaptureState::Running {
            next_offset,
            sender,
            ..
        } = &mut self.capture
        else {
            unreachable!()
        };
        if sender.send(CaptureMessage::Bytes(bytes.to_vec())).is_err() {
            self.invalidate_capture();
            return;
        }
        *next_offset = match next_offset.checked_add(bytes.len() as u64) {
            Some(next) => next,
            None => {
                self.invalidate_capture();
                return;
            }
        };
    }

    pub(crate) fn finish_capture(&mut self, node: Option<NodeId>) {
        if !matches!(
            &self.capture,
            CaptureState::Running { node: active, .. }
                if node.is_none_or(|node| node == *active)
        ) {
            return;
        }
        let state = std::mem::replace(&mut self.capture, CaptureState::Invalid);
        let CaptureState::Running {
            node,
            next_offset,
            sender,
            thread,
        } = state
        else {
            unreachable!()
        };
        let finished = sender.send(CaptureMessage::Finish).is_ok();
        drop(sender);
        if let (true, Ok(Ok(content))) = (finished, thread.join()) {
            self.capture = CaptureState::Ready(Box::new(CapturedFile {
                node,
                len: next_offset,
                root: content.root,
                counters: content.counters,
                objects: content.objects,
            }));
        }
    }

    pub(crate) fn invalidate_capture(&mut self) {
        let state = std::mem::replace(&mut self.capture, CaptureState::Invalid);
        if let CaptureState::Running { sender, thread, .. } = state {
            let _ = sender.send(CaptureMessage::Abort);
            drop(sender);
            let _ = thread.join();
        }
    }

    pub(crate) fn take_capture(&mut self) -> Option<CapturedFile> {
        self.finish_capture(None);
        let CaptureState::Ready(captured) =
            std::mem::replace(&mut self.capture, CaptureState::Invalid)
        else {
            return None;
        };
        let exact = matches!(
            self.nodes.get(&captured.node).map(|node| &node.data),
            Some(Data::File(FileData::Overlay {
                base: None,
                len,
                dirty,
                charged,
                ..
            })) if *len == captured.len
                && dirty.len() == 1
                && dirty.get(&0) == Some(len)
                && charged.len() == 1
                && charged.get(&0) == Some(len)
        );
        exact.then_some(*captured)
    }

    fn start_capture(&mut self, node: NodeId) -> std::io::Result<()> {
        let (sender, receiver) = sync_channel(1);
        let thread = std::thread::Builder::new()
            .name("layerfs-capture".to_owned())
            .spawn(move || build_capture(receiver))?;
        self.capture = CaptureState::Running {
            node,
            next_offset: 0,
            sender,
            thread,
        };
        Ok(())
    }
}

impl Read for CaptureReader {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        if output.is_empty() || self.done {
            return Ok(0);
        }
        loop {
            let read = self.current.read(output)?;
            if read != 0 {
                return Ok(read);
            }
            match self.receiver.recv() {
                Ok(CaptureMessage::Bytes(bytes)) => self.current = Cursor::new(bytes),
                Ok(CaptureMessage::Finish) => {
                    self.done = true;
                    return Ok(0);
                }
                Ok(CaptureMessage::Abort) | Err(_) => {
                    return Err(std::io::Error::other("capture aborted"));
                }
            }
        }
    }
}

fn build_capture(receiver: Receiver<CaptureMessage>) -> Result<CapturedContent> {
    let mut objects = ObjectBuffer::empty()?;
    let reader = CaptureReader {
        receiver,
        current: Cursor::new(Vec::new()),
        done: false,
    };
    let (root, counters) = rope::build(&mut objects, reader)?;
    Ok(CapturedContent {
        root,
        counters,
        objects: objects.into_prevalidated()?,
    })
}
