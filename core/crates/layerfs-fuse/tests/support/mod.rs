use layerfs_fuse::{Dispatch, DispatchConfig, MountQueue, RequestDisposition};
use layerfs_overlay::{NativeMount, Overlay, ProfileConfig};
use std::{
    future::Future,
    path::PathBuf,
    pin::Pin,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    task::{Context, Poll, Waker},
    time::{Duration, Instant},
};

pub struct Fixture {
    db: Overlay,
    path: PathBuf,
}
impl Fixture {
    pub fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let path = std::env::temp_dir().join(format!(
            "layerfs-dispatch-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        let db = Overlay::create(&path.join("overlay"), ProfileConfig::default()).unwrap();
        Self { db, path }
    }
    pub fn mount(&self, id: u8) -> NativeMount {
        let route = self.db.open_workspace([id; 32], [id; 32]).unwrap();
        self.db.create_native_mount(route, 1).unwrap()
    }
    pub fn pool(&self, namespaces: usize) -> Dispatch {
        Dispatch::start(DispatchConfig {
            read_handles: 2,
            namespaces,
        })
        .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

pub fn until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !condition() {
        assert!(Instant::now() < deadline, "bounded observation expired");
        std::thread::yield_now();
    }
}
pub fn finish(queue: &MountQueue) {
    until(|| queue.work().unwrap().admitted == 0);
    queue.stop_admission().unwrap();
    queue.finish().unwrap();
}

#[derive(Default)]
pub struct Event(Mutex<(bool, Option<Waker>)>);
impl Event {
    pub fn fire(&self) {
        let wake = {
            let mut state = self.0.lock().unwrap();
            state.0 = true;
            state.1.take()
        };
        if let Some(wake) = wake {
            wake.wake();
        }
    }
    pub fn future(self: &Arc<Self>) -> EventFuture {
        EventFuture(self.clone())
    }
    pub fn waker(&self) -> Option<Waker> {
        self.0.lock().unwrap().1.clone()
    }
}
pub struct EventFuture(Arc<Event>);
impl Future for EventFuture {
    type Output = RequestDisposition;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = self.0 .0.lock().unwrap();
        if state.0 {
            Poll::Ready(RequestDisposition::Complete)
        } else {
            state.1 = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}
