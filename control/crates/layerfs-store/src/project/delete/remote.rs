//! The worker-side opens of `project.delete`: the remote effects and the
//! `OpenArgs`/Stack lookups a local Store call needs.

use super::*;

pub(super) struct Remote {
    pub(super) worker: Worker,
    pub(super) store: String,
    pub(super) project: String,
    pub(super) store_record: SdkValue,
    /// The bounded places the caller's home can speak for, read once for the
    /// remove. The worker installs it around each admission; a worker whose
    /// home is not the caller's stays conservative live.
    pub(super) place: Option<SdkValue>,
}

/// Adds the bounded place view to a worker frame. A frame that is not a map
/// is returned unchanged: the worker then reads no place.
fn with_place(frame: SdkValue, place: Option<&SdkValue>) -> SdkValue {
    match frame {
        SdkValue::Map(mut fields) => {
            if let Some(place) = place {
                fields.push(("place_view".to_owned(), place.clone()));
            }
            SdkValue::Map(fields)
        }
        other => other,
    }
}

impl Effects for Remote {
    fn admit(&mut self, stack: LayerStackId, op: [u8; 16], project: &str) -> Result<Admission> {
        let frame = DeleteAdmitFrame {
            store: StoreName::parse(&self.store)?,
            stack,
            op,
            // the row is stored: naming it makes a later `Busy`/`Unresolved`
            // report the project rather than the hex LayerStack id
            project: Some(project.to_owned()),
            store_record: Some(self.store_record.clone()),
        };
        let answer = match self.worker.call(
            "project.delete.admit",
            with_place(frame.encode(), self.place.as_ref()),
            &mut |_| {},
        ) {
            Ok(answer) => answer,
            Err(refusal) => {
                // The request went out, so the worker may have registered the
                // row before its answer was lost: settle the identity before
                // reporting the unknown outcome.
                if refusal.code == "Unknown" {
                    let _ = self.release(Admission::new(stack, op));
                }
                return Err(crate::unknown::named_unknown(
                    refusal,
                    "project",
                    &self.project,
                    Some(&op),
                ));
            }
        };
        if let Err(refusal) = admit_reply(&answer, stack) {
            let _ = self.release(Admission::new(stack, op));
            return Err(refusal);
        }
        Ok(Admission::new(stack, op))
    }

    fn release(&mut self, admission: Admission) -> Result<()> {
        let frame = DeleteReleaseFrame {
            store: StoreName::parse(&self.store)?,
            op: admission.op,
            store_record: Some(self.store_record.clone()),
        };
        self.worker
            .call("project.delete.release", frame.encode(), &mut |_| {})
            .map_err(|refusal| {
                crate::unknown::named_unknown(
                    refusal,
                    "project",
                    &self.project,
                    Some(&admission.op),
                )
            })?;
        Ok(())
    }

    fn mark(
        &mut self,
        admission: Admission,
        deleted_ms: i64,
        pending_retry: bool,
    ) -> Result<Deletion> {
        let frame = DeleteFrame {
            store: StoreName::parse(&self.store)?,
            project: Some(self.project.clone()),
            stack: admission.stack,
            deleted_ms: Some(deleted_ms),
            op_id: Some(admission.op),
            pending_retry: Some(pending_retry),
            store_record: Some(self.store_record.clone()),
        };
        let answer = self
            .worker
            .call(
                "project.delete",
                with_place(frame.encode(), self.place.as_ref()),
                &mut |_| {},
            )
            .map_err(|refusal| {
                crate::unknown::named_unknown(
                    refusal,
                    "project",
                    &self.project,
                    Some(&admission.op),
                )
            })?;
        let reply = DeleteReply::decode(&answer)?;
        if reply.stack != admission.stack {
            return Err(refused(
                "Failed",
                "the worker answered for another LayerStack",
            ));
        }
        Ok(if reply.retired == Some(true) {
            Deletion::Retired {
                deleted_ms: reply.deleted_ms,
            }
        } else {
            Deletion::Marked {
                deleted_ms: reply.deleted_ms,
            }
        })
    }

    fn settle(&mut self, stack: LayerStackId) -> Result<()> {
        let frame = DeleteEntryFrame {
            store: StoreName::parse(&self.store)?,
            stack,
            project: Some(self.project.clone()),
        };
        let answer = self
            .worker
            .call("project.delete.entry", frame.encode(), &mut |_| {})
            .map_err(|refusal| {
                // no admission identity is held on this path
                crate::unknown::named_unknown(refusal, "project", &self.project, None)
            })?;
        let reply = DeleteEntryReply::decode(&answer)?;
        if reply.stack != stack {
            return Err(refused(
                "Failed",
                "the worker answered for another LayerStack",
            ));
        }
        Ok(())
    }

    fn remove_entry(&mut self, entry: &Path) -> Result<()> {
        remove_tree(entry)
    }
}

/// Checks the worker's `project.delete.admit` answer.
pub(super) fn admit_reply(answer: &SdkValue, stack: LayerStackId) -> Result<()> {
    if answer.get_text("kind") != Some("Admitted") {
        return Err(refused(
            "Failed",
            "the worker project.delete admission is not `Admitted`",
        ));
    }
    let answered = answer
        .get_text("stack_id")
        .and_then(unhex)
        .and_then(|bytes| LayerStackId::from_slice(&bytes).ok());
    if answered != Some(stack) {
        return Err(refused("Failed", "the worker admitted another LayerStack"));
    }
    Ok(())
}

/// Names the project in a `project.delete` Store-listing refusal whose detail
/// is empty: `Unknown` from a bare `UnknownOutcome` or a poisoned Store
/// object, rather than `Unknown {}` (F43R5-2). `listed_stack` wraps both
/// callers' failures with this.
pub fn listed_unknown(refusal: SdkRefusal, name: &str) -> SdkRefusal {
    crate::unknown::named_unknown(refusal, "project", name, None)
}

/// The Stack id a Store lists for project `name`, for a record that does
/// not name one (spec §4.2 fills it from the filtered listing by name).
pub(super) fn listed_stack(open: &OpenArgs, name: &str) -> Result<Option<LayerStackId>> {
    service::with_service(open, |service| {
        let stacks = service
            .store
            .published(layerfs_project::store::projects)
            .map_err(lookup::published)?;
        Ok(stacks
            .into_iter()
            .find(|stack| stack.name.as_str() == name)
            .map(|stack| stack.id))
    })
    // A listing that cannot be determined still knows the project it asked
    // about (F43R5-2).
    .map_err(|refusal| listed_unknown(refusal, name))
}

/// The Stack id the Store on another machine lists for project `name`, for
/// a record that names none (an Init through the worker whose outcome is
/// not known): the Store's own list, read once in its worker. `None` when
/// the Store does not hold it; a lost answer is `Unknown`, naming the
/// project, and nothing is removed.
pub(super) fn listed_on_worker(
    home: &Path,
    name: &str,
    record: &StoreRecord,
) -> Result<Option<LayerStackId>> {
    let projects = crate::project::live::projects(home, record.name(), record, false)
        .map_err(|refusal| listed_unknown(refusal, name))?;
    Ok(projects
        .into_iter()
        .find(|project| project.name == name)
        .map(|project| project.stack))
}
