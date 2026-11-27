//! One-ID provider windows with the caller's actual demand timing.

use layerfs_telemetry::timer::{Active, TimingScope};

use crate::error::{ContentError, ContentResult};
use crate::object::{AuthenticatedObjects, ObjectId};

pub(super) struct Reader<'a, 'b> {
    provider: &'a dyn AuthenticatedObjects,
    scope: &'a TimingScope<'b, Active>,
}

impl<'a, 'b> Reader<'a, 'b> {
    pub(super) fn new(
        provider: &'a dyn AuthenticatedObjects,
        scope: &'a TimingScope<'b, Active>,
    ) -> Self {
        Self { provider, scope }
    }
}

impl AuthenticatedObjects for Reader<'_, '_> {
    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        let mut result = Vec::with_capacity(ids.len());
        for id in ids {
            let mut values = self.provider.read_canonical_batch_scoped(
                std::slice::from_ref(id),
                self.scope.child("content.context.read"),
            )?;
            if values.len() != 1 {
                return Err(ContentError::BatchCardinality {
                    requested: 1,
                    returned: values.len(),
                });
            }
            result.push(values.pop().ok_or(ContentError::BatchCardinality {
                requested: 1,
                returned: 0,
            })?);
        }
        Ok(result)
    }
}
