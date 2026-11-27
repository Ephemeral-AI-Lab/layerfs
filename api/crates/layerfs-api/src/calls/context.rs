//! What one running call can reach: its home and its one event stream
//! (moved back from layerfs-home by layerfs-project#107; the entities take
//! the home and [`Events`] apart).
use super::form::Asked;
use layerfs_home::events::Events;
use layerfs_protocol::sdk::SdkValue;
use std::path::{Path, PathBuf};

/// What one running call can reach: its home and its event stream.
pub struct Context<'a> {
    /// The home every record lives in.
    pub home: PathBuf,
    /// The tag of the call's log line, if it has one (`log_tag`, else
    /// `LAYERFS_LOGS_TAG`): `workspace.exec` keeps it in its command's
    /// record.
    pub tag: Option<String>,
    asked: Asked,
    events: Events<'a>,
}

impl<'a> Context<'a> {
    /// A call's context: its home, its log tag, what the caller asked of
    /// the answer (`detail`, `with`) and its one event stream.
    pub fn new(
        home: PathBuf,
        tag: Option<String>,
        asked: Asked,
        events: &'a mut dyn FnMut(SdkValue),
    ) -> Self {
        Context {
            home,
            tag,
            asked,
            events: Events::new(events),
        }
    }

    /// Whether the caller asked for section `name` (`--with`): a run
    /// function does that section's read only then.
    pub fn with(&self, name: &str) -> bool {
        self.asked.with(name)
    }

    /// The home and the event stream at once, for an entity function that
    /// takes both.
    pub fn parts(&mut self) -> (&Path, &mut Events<'a>) {
        (&self.home, &mut self.events)
    }
}
