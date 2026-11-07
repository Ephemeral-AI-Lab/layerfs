//! Typed record fields in file order (layerfs-project#108): the record
//! codec's typed half. An entity declares the fields its record knows as
//! one [`Field`] enum; a record reads as those, each typed, with every
//! other field (one this build does not know, or one whose value is not of
//! its field's type) kept exactly as read, and the file's order kept. A
//! record read and written back is the same JSON, field for field, null for
//! null; a changed field keeps its place and a new one comes last, as
//! [`super::with_field`] places them. It knows no entity.
use layerfs_bridge::control::SdkValue;

/// The fields one entity's record knows, each with its type.
pub trait Field: Sized + Clone {
    /// The known field `name` holding `value`; `None` when `name` is not
    /// known or `value` is not of its type, and the field is kept as read.
    fn decode(name: &str, value: &SdkValue) -> Option<Self>;
    /// Its name in the record.
    fn name(&self) -> &'static str;
    /// Its value in the record.
    fn encode(&self) -> SdkValue;
}

/// One field of a record: known and typed, or kept as read.
#[derive(Clone, Debug, PartialEq)]
enum Slot<F> {
    Known(F),
    Kept(String, SdkValue),
}

impl<F: Field> Slot<F> {
    fn name(&self) -> &str {
        match self {
            Self::Known(field) => field.name(),
            Self::Kept(name, _) => name,
        }
    }
}

/// A record's fields, in file order.
#[derive(Clone, Debug, PartialEq)]
pub struct Fields<F> {
    slots: Vec<Slot<F>>,
}

impl<F: Field> Default for Fields<F> {
    fn default() -> Self {
        Self { slots: Vec::new() }
    }
}

impl<F: Field> Fields<F> {
    /// The fields of record `value`; a value that is not a map has none.
    pub fn decode(value: &SdkValue) -> Self {
        let SdkValue::Map(fields) = value else {
            return Self::default();
        };
        let slots = (fields.iter())
            .map(|(name, value)| match F::decode(name, value) {
                Some(field) => Slot::Known(field),
                None => Slot::Kept(name.clone(), value.clone()),
            })
            .collect();
        Self { slots }
    }

    /// The fields `known`, in that order.
    pub fn of(known: impl IntoIterator<Item = F>) -> Self {
        Self {
            slots: known.into_iter().map(Slot::Known).collect(),
        }
    }

    /// The record: one map, in file order.
    pub fn encode(&self) -> SdkValue {
        SdkValue::Map(
            (self.slots.iter())
                .map(|slot| match slot {
                    Slot::Known(field) => (field.name().to_owned(), field.encode()),
                    Slot::Kept(name, value) => (name.clone(), value.clone()),
                })
                .collect(),
        )
    }

    /// The first known field `pick` takes.
    pub fn find<'a, T: ?Sized>(&'a self, pick: impl Fn(&'a F) -> Option<&'a T>) -> Option<&'a T> {
        self.slots.iter().find_map(|slot| match slot {
            Slot::Known(field) => pick(field),
            Slot::Kept(..) => None,
        })
    }

    /// The first known field `pick` takes, by value.
    pub fn get<T>(&self, pick: impl Fn(&F) -> Option<T>) -> Option<T> {
        self.slots.iter().find_map(|slot| match slot {
            Slot::Known(field) => pick(field),
            Slot::Kept(..) => None,
        })
    }

    /// The field `name` as read, when this build does not know it or its
    /// value is not of its type.
    pub fn kept(&self, name: &str) -> Option<&SdkValue> {
        self.slots.iter().find_map(|slot| match slot {
            Slot::Kept(kept, value) if kept == name => Some(value),
            _ => None,
        })
    }

    /// Whether the record has a field `name`, of any value.
    pub fn has(&self, name: &str) -> bool {
        self.slots.iter().any(|slot| slot.name() == name)
    }

    /// `field` set: in its place when the record has its name, else last.
    pub fn set(&mut self, field: F) {
        match self
            .slots
            .iter_mut()
            .find(|slot| slot.name() == field.name())
        {
            Some(slot) => *slot = Slot::Known(field),
            None => self.slots.push(Slot::Known(field)),
        }
    }

    /// Field `name` set to `value` as it arrived (a call's answer): typed
    /// when it is a known field of its type, else kept as it is.
    pub fn put(&mut self, name: &str, value: SdkValue) {
        let slot = match F::decode(name, &value) {
            Some(field) => Slot::Known(field),
            None => Slot::Kept(name.to_owned(), value),
        };
        match self.slots.iter_mut().find(|old| old.name() == name) {
            Some(old) => *old = slot,
            None => self.slots.push(slot),
        }
    }

    /// The record without field `name`.
    pub fn remove(&mut self, name: &str) {
        self.slots.retain(|slot| slot.name() != name);
    }

    /// The fields but those named in `hidden`, as a map in file order.
    pub fn encode_without(&self, hidden: &[&str]) -> Vec<(String, SdkValue)> {
        match self.encode() {
            SdkValue::Map(fields) => (fields.into_iter())
                .filter(|(name, _)| !hidden.contains(&name.as_str()))
                .collect(),
            _ => Vec::new(),
        }
    }
}

/// A text field's value, stored as text or null.
pub fn opt_text(value: &SdkValue) -> Option<Option<String>> {
    match value {
        SdkValue::Text(text) => Some(Some(text.clone())),
        SdkValue::Null => Some(None),
        _ => None,
    }
}

/// A text field's value, stored as text.
pub fn text(value: &SdkValue) -> Option<String> {
    match value {
        SdkValue::Text(text) => Some(text.clone()),
        _ => None,
    }
}

/// An integer field's value.
pub fn int(value: &SdkValue) -> Option<i64> {
    match value {
        SdkValue::Int(n) => Some(*n),
        _ => None,
    }
}
