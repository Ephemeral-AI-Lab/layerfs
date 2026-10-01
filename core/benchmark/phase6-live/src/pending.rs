//! Private finalized output owned by exactly one construction operation.
use layerfs_content::{FinalizedObject, ObjectId};
use std::collections::BTreeMap;
pub const OBJECTS: usize = 512;
pub const BYTES: usize = 4 * 1024 * 1024 - 1;
#[derive(Default)]
pub struct Pending {
    pub objects: Vec<FinalizedObject>,
    index: BTreeMap<ObjectId, usize>,
    pub bytes: usize,
    pub peak_objects: usize,
    pub peak_bytes: usize,
}
impl Pending {
    pub fn get(&self, id: ObjectId) -> Option<&FinalizedObject> {
        self.index.get(&id).map(|i| &self.objects[*i])
    }
    pub fn fits(&self, n: usize) -> bool {
        self.objects.len() < OBJECTS && self.bytes.checked_add(n).is_some_and(|b| b <= BYTES)
    }
    pub fn push(&mut self, object: FinalizedObject) -> Result<bool, String> {
        if let Some(old) = self.get(object.id()) {
            if old.role() != object.role() || old.canonical() != object.canonical() {
                return Err("private exact CAS mismatch".into());
            }
            return Ok(false);
        }
        if !self.fits(object.canonical_len()) {
            return Err("private pending admission".into());
        }
        self.bytes += object.canonical_len();
        self.index.insert(object.id(), self.objects.len());
        self.objects.push(object);
        self.peak_objects = self.peak_objects.max(self.objects.len());
        self.peak_bytes = self.peak_bytes.max(self.bytes);
        Ok(true)
    }
    pub fn drain(&mut self) -> Vec<FinalizedObject> {
        self.bytes = 0;
        self.index.clear();
        std::mem::take(&mut self.objects)
    }
}
