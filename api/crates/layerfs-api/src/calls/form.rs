//! What a call's answer holds (call surface 2.0): the keys of its short
//! form and of its long form, the sections `--with` can add, how it pages
//! and which profile lists it. A table entry declares these as `const`
//! data; [`super::shape`] makes every answer from them, so a run function
//! builds one full map and never two forms. An entry without a [`Form`] is
//! not migrated yet: its result passes through as it is.
use layerfs_bridge::control::SdkValue;

/// The keys of one answer map: a call's top level, an item of its list, a
/// section, or a node of a tree.
#[derive(Clone, Copy, Debug)]
pub struct Form {
    /// The short keys present in every answer (left out only when null).
    pub always: &'static [&'static str],
    /// The short keys present only when they are news: not null, false, or
    /// an empty text, list or map.
    pub sometimes: &'static [&'static str],
    /// The complete, ordered keys of the long form; every short key is one.
    pub long: &'static [&'static str],
    /// The keys whose value has a form of its own.
    pub parts: &'static [(&'static str, Part)],
    /// The usual value of a `sometimes` key, which is not news: a Store's
    /// `engine` is `sqlite` unless it says otherwise. (A `state` of `ready`
    /// is never news in a `sometimes` key; no form needs to say so.)
    pub usual: &'static [(&'static str, &'static str)],
    /// The words that may name the map's thing where its keys say
    /// [`THING`]: the map holds exactly one of them, and both forms show
    /// it under its own word (a doctor's finding names a machine, a Store
    /// or a Workspace alike).
    pub things: &'static [&'static str],
    /// The `sometimes` keys the short form shows only when a test of the
    /// whole map says so: `file.read`'s `size` only beside `binary`. The
    /// long form is not changed.
    pub when: &'static [(&'static str, ShortWhen)],
}

/// Whether the short form shows a key, from the whole map the run function
/// answered (its fields as given) and the names of the sections asked for.
pub type ShortWhen = fn(&[(String, SdkValue)], &[&str]) -> bool;

/// `observed_at` in a records-only read's short form: only beside the
/// `unknown` state it explains, or when `--with live` was asked (conformance
/// R3 F3). The long form keeps it.
pub const OBSERVED: (&str, ShortWhen) = ("observed_at", observed);

fn observed(fields: &[(String, SdkValue)], asked: &[&str]) -> bool {
    asked.contains(&"live")
        || (fields.iter()).any(|(key, value)| key == "state" && *value == SdkValue::text("unknown"))
}

/// What a key with its own form holds.
#[derive(Clone, Copy, Debug)]
pub enum Part {
    /// A map, or a list of maps, each shaped by this form.
    Rows(&'static Form),
    /// A list of plain strings; the word says what each string is.
    Scalars(&'static str),
}

impl Form {
    /// A form with these short keys and long keys, and no part.
    pub const fn of(
        always: &'static [&'static str],
        sometimes: &'static [&'static str],
        long: &'static [&'static str],
    ) -> Form {
        Form {
            always,
            sometimes,
            long,
            parts: &[],
            usual: &[],
            things: &[],
            when: &[],
        }
    }

    /// This form, each of these `sometimes` keys shown in the short form
    /// only when its test of the whole map holds ([`Form::when`]).
    pub const fn short_when(self, when: &'static [(&'static str, ShortWhen)]) -> Form {
        Form { when, ..self }
    }

    /// This form, its [`THING`] key held by one of `things`.
    pub const fn things(self, things: &'static [&'static str]) -> Form {
        Form { things, ..self }
    }

    /// This form, each of these `sometimes` keys left out of the short
    /// form while it holds its usual value.
    pub const fn usual(self, usual: &'static [(&'static str, &'static str)]) -> Form {
        Form { usual, ..self }
    }

    /// This form, with `parts` shaped by their own forms.
    pub const fn parts(self, parts: &'static [(&'static str, Part)]) -> Form {
        Form { parts, ..self }
    }

    /// The form of the value under `key`, when it has its own.
    pub fn part(&self, key: &str) -> Option<&Part> {
        self.parts
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, part)| part)
    }

    /// The form of each item of a list call (`items`), when its rows are maps.
    pub fn item(&self) -> Option<&'static Form> {
        match self.part(ITEMS) {
            Some(Part::Rows(form)) => Some(form),
            _ => None,
        }
    }
}

/// The key that stands for the thing a map names, by its own word (see
/// [`Form::things`]).
pub const THING: &str = "<thing>";
/// The key a list call answers its rows under.
pub const ITEMS: &str = "items";
/// The key of an answer with nothing else to say.
pub const OK: &str = "ok";

/// Where a section's keys go in the answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    /// Under the section's own name, as one map.
    Own,
    /// Flat on each item of the call's list.
    Item,
}

impl Place {
    /// The place as the specification's data names it.
    pub fn word(self) -> &'static str {
        match self {
            Place::Own => "own",
            Place::Item => "item",
        }
    }
}

/// One section `--with` can add to a call's answer.
#[derive(Clone, Copy, Debug)]
pub struct Section {
    /// Its name, the `--with` value.
    pub name: &'static str,
    /// Where its keys go.
    pub place: Place,
    /// Its keys.
    pub keys: &'static [&'static str],
    /// The form of its own map, when its rows are shaped; without one its
    /// map holds exactly `keys`.
    pub form: Option<&'static Form>,
}

impl Section {
    /// A section answered under its own name.
    pub const fn own(name: &'static str, keys: &'static [&'static str]) -> Section {
        Section {
            name,
            place: Place::Own,
            keys,
            form: None,
        }
    }

    /// A section whose keys go flat on each item.
    pub const fn item(name: &'static str, keys: &'static [&'static str]) -> Section {
        Section {
            place: Place::Item,
            ..Section::own(name, keys)
        }
    }

    /// This section, its own map shaped by `form`.
    pub const fn shaped(self, form: &'static Form) -> Section {
        Section {
            form: Some(form),
            ..self
        }
    }
}

/// How a call pages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Paging {
    /// One bounded answer.
    None,
    /// `--limit` and `--cursor` in, `items` and `next_cursor` out.
    Cursor,
    /// A window of text lines: `--line` and `--limit` in, `next_line` out.
    Line,
}

impl Paging {
    /// The paging as the specification's data names it; none is null there.
    pub fn word(self) -> Option<&'static str> {
        match self {
            Paging::None => None,
            Paging::Cursor => Some("cursor"),
            Paging::Line => Some("line"),
        }
    }
}

/// Which profile lists a call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    /// The agent profile: a tool an agent is offered.
    Agent,
    /// The full table only: a UI's or an operator's call.
    Full,
}

impl Profile {
    /// The profile as the specification's data names it.
    pub fn word(self) -> &'static str {
        match self {
            Profile::Agent => "agent",
            Profile::Full => "full",
        }
    }
}

/// What the caller asked of the answer: the global options `detail` and
/// `with`, taken out of a call's arguments before they are checked.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Asked {
    /// The long form.
    pub detail: bool,
    /// The sections asked for, each one the call declares.
    pub with: Vec<String>,
}

impl Asked {
    /// Whether section `name` was asked for.
    pub fn with(&self, name: &str) -> bool {
        self.with.iter().any(|asked| asked == name)
    }
}
