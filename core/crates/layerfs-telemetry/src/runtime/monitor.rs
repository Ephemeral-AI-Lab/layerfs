//! One process sampler with a fixed recent ring and non-evicting active summaries.
use crate::observation::{Observation, Window, WindowSource};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

/// Explicit native configuration; constructing/importing types starts no work.
#[derive(Clone, Copy)]
pub struct MonitorConfig {
    /// Select cumulative CPU observations.
    pub cpu: bool,
    /// Select resident memory observations.
    pub memory: bool,
    /// Sampling interval; accepted 10..=1000 milliseconds.
    pub interval_ms: u64,
    /// Recent sample slots, 1..=600.
    pub history: usize,
    /// Live summary slots, 1..=32.
    pub windows: usize,
}
impl MonitorConfig {
    pub(crate) fn validate(&self) -> std::io::Result<()> {
        if !(10..=1000).contains(&self.interval_ms)
            || self.history == 0
            || self.history > 600
            || self.windows == 0
            || self.windows > 32
        {
            return Err(std::io::Error::other("monitor limits"));
        }
        Ok(())
    }
}
struct Slot {
    generation: AtomicU64,
    active: AtomicBool,
    summary: Mutex<Window>,
}
struct State {
    slots: Vec<Slot>,
    history: Mutex<(Vec<Option<Observation>>, usize)>,
    stop: AtomicBool,
    failures: crate::health::Counter,
    origin: Instant,
}
/// Native lifetime owner. Its window handle can be shared across hosted components.
pub struct Monitor {
    state: Arc<State>,
    worker: Option<JoinHandle<()>>,
}
impl Monitor {
    /// Validates all counts before allocating. Deselected resources create no sampler.
    pub fn start(config: MonitorConfig) -> std::io::Result<Option<Self>> {
        Self::start_with_output(config, None)
    }
    /// Starts one monitor with an optional bounded periodic-output handoff.
    pub fn start_with_output(
        config: MonitorConfig,
        output: Option<(crate::output::Output, crate::output::Identity)>,
    ) -> std::io::Result<Option<Self>> {
        if !config.cpu && !config.memory {
            return Ok(None);
        }
        config.validate()?;
        let state = Arc::new(State {
            slots: (0..config.windows)
                .map(|_| Slot {
                    generation: AtomicU64::new(0),
                    active: AtomicBool::new(false),
                    summary: Mutex::new(Window::default()),
                })
                .collect(),
            history: Mutex::new((vec![None; config.history], 0)),
            stop: AtomicBool::new(false),
            failures: crate::health::Counter::new(),
            origin: Instant::now(),
        });
        let owner = Arc::clone(&state);
        let worker = thread::Builder::new()
            .name("layerfs-observe".into())
            .stack_size(256 * 1024)
            .spawn(move || {
                let start = owner.origin;
                let mut due = start;
                let mut emission = start;
                while !owner.stop.load(Ordering::Acquire) {
                    let now = Instant::now();
                    if now < due {
                        thread::park_timeout(due - now);
                        continue;
                    }
                    let missed =
                        now.duration_since(due).as_millis() / u128::from(config.interval_ms);
                    let at_ns = u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX);
                    let probe_start=Instant::now();
                    let sample = crate::platform::sample(at_ns, config.cpu, config.memory).map(|mut sample|{
                        sample.probe_ns=u64::try_from(probe_start.elapsed().as_nanos()).ok();
                        sample.at_ns=u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX);
                        sample
                    });
                    if sample.is_none() {
                        owner.failures.add(1);
                    }
                    if now >= emission {
                        if let Some((out,identity))=&output {
                            if let Some(sample)=sample {out.submit(crate::output::encode_resource(*identity,sample));}
                            else {out.submit(format!("LFT1 {{\"v\":1,\"kind\":\"resource-unavailable\",\"run\":\"{:032x}\",\"namespace\":{},\"pid\":{},\"role\":{},\"source\":\"{}\",\"selected\":{},\"failures\":{},\"overflow\":{}}}\n",identity.run,identity.namespace,identity.pid,identity.role,crate::platform::SOURCE.as_str(),u8::from(config.cpu) | (u8::from(config.memory) << 1),owner.failures.value(),owner.failures.overflowed()).into_bytes());}
                        }
                        emission = now + Duration::from_secs(1);
                    }
                    let active = owner
                        .slots
                        .iter()
                        .filter(|s| s.active.load(Ordering::Acquire))
                        .count();
                    for slot in &owner.slots {
                        let generation = slot.generation.load(Ordering::Acquire);
                        if !slot.active.load(Ordering::Acquire) {
                            continue;
                        }
                        if let Ok(mut summary) = slot.summary.try_lock() {
                            if slot.active.load(Ordering::Acquire)
                                && slot.generation.load(Ordering::Acquire) == generation
                            {
                                if let Some(sample) = sample {
                                    summary.observe(sample, active);
                                }
                                summary.gaps = summary.gaps.saturating_add(
                                    (missed.min(u64::MAX as u128) as u64)
                                        .saturating_add(u64::from(sample.is_none())),
                                );
                            }
                        }
                    }
                    if let Ok(mut ring) = owner.history.try_lock() {
                        let index = ring.1;
                        ring.0[index] = sample;
                        ring.1 = (index + 1) % ring.0.len();
                    }
                    let next_tick =
                        start.elapsed().as_millis() / u128::from(config.interval_ms) + 1;
                    let Some(next_ms) = next_tick
                        .checked_mul(u128::from(config.interval_ms))
                        .and_then(|n| u64::try_from(n).ok())
                    else {
                        owner.failures.add(1);
                        break;
                    };
                    due = start + Duration::from_millis(next_ms);
                }
            })?;
        Ok(Some(Self {
            state,
            worker: Some(worker),
        }))
    }
    /// Shared optional-window capability; does not create a second sampler.
    pub fn windows(&self) -> Arc<dyn WindowSource> {
        self.state.clone()
    }
    /// Failed native observations, never interpreted as zero CPU or zero memory.
    pub fn failures(&self) -> u64 {
        self.state.failures.value()
    }
    /// Latest observation, if available without waiting for a lock.
    pub fn latest(&self) -> Option<Observation> {
        let ring = self.state.history.try_lock().ok()?;
        ring.0[(ring.1 + ring.0.len() - 1) % ring.0.len()]
    }
}
impl Drop for Monitor {
    fn drop(&mut self) {
        self.state.stop.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            worker.thread().unpark();
            if worker.is_finished() {
                let _ = worker.join();
            }
        }
    }
}
impl WindowSource for State {
    fn begin(&self) -> Option<u64> {
        if self.stop.load(Ordering::Acquire) {
            return None;
        }
        for (index, slot) in self.slots.iter().enumerate() {
            if slot
                .active
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_err()
            {
                continue;
            }
            let generation =
                match slot
                    .generation
                    .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                        n.checked_add(1).filter(|n| *n <= u64::MAX / 64)
                    }) {
                    Ok(old) => old + 1,
                    Err(_) => {
                        slot.active.store(false, Ordering::Release);
                        return None;
                    }
                };
            if let Ok(mut summary) = slot.summary.try_lock() {
                *summary = Window {
                    opened_ns: u64::try_from(self.origin.elapsed().as_nanos()).ok(),
                    ..Window::default()
                };
                if let Ok(ring) = self.history.try_lock() {
                    if let Some(sample) = ring.0[(ring.1 + ring.0.len() - 1) % ring.0.len()] {
                        summary.observe(sample, 1);
                    }
                }
                return Some(generation * 64 + index as u64);
            }
            slot.active.store(false, Ordering::Release);
        }
        None
    }
    fn finish(&self, token: u64) -> Option<Window> {
        let slot = self.slots.get((token % 64) as usize)?;
        let summary = slot.summary.try_lock().ok()?;
        if slot.generation.load(Ordering::Acquire) != token / 64
            || !slot.active.load(Ordering::Acquire)
        {
            return None;
        }
        let mut report = *summary;
        report.closed_ns = u64::try_from(self.origin.elapsed().as_nanos()).ok();
        Some(report)
    }
    fn release(&self, token: u64) {
        if let Some(slot) = self.slots.get((token % 64) as usize) {
            if slot.generation.load(Ordering::Acquire) == token / 64 {
                slot.active.store(false, Ordering::Release);
            }
        }
    }
}
