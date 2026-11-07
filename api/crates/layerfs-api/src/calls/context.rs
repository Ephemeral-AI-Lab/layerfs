//! What one running call can reach: its home and its one event stream
//! (moved back from layerfs-home by layerfs-project#107; the entities take
//! the home and [`Events`] apart).
use layerfs_bridge::control::SdkValue;
use layerfs_home::events::Events;
use std::path::{Path, PathBuf};

/// What one running call can reach: its home and its event stream.
pub struct Context<'a> {
    /// The home every record lives in.
    pub home: PathBuf,
    events: Events<'a>,
}

impl<'a> Context<'a> {
    /// A call's context: its home and its one event stream.
    pub fn new(home: PathBuf, events: &'a mut dyn FnMut(SdkValue)) -> Self {
        Context {
            home,
            events: Events::new(events),
        }
    }

    /// The call's event stream, for the entity function it runs.
    pub fn events(&mut self) -> &mut Events<'a> {
        &mut self.events
    }

    /// The home and the event stream at once, for an entity function that
    /// takes both.
    pub fn parts(&mut self) -> (&Path, &mut Events<'a>) {
        (&self.home, &mut self.events)
    }
}
