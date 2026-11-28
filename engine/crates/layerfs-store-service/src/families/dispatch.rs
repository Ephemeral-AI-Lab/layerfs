//! The public method table: each 2.0 Store call name runs its method file
//! once. Operator names are absent from this table.
use crate::families::args::{self, Answer};
use crate::families::{branch, commit, file, project, store};
use crate::filesystem::change::run::Source;
use crate::StoreService;
use layerfs_protocol::sdk::{SdkCall, SdkValue};

/// Runs one public Store method once, in this process, on `service`: one
/// call, so a quarantined PostgreSQL session reopens at most once in it.
pub fn call(service: &StoreService, call: &SdkCall) -> Answer {
    layerfs_persistence::one_call(|| call_once(service, call))
}

/// Runs `file.write` or `file.apply` once on `service` with its content or its
/// changes pulled from `source` as they are applied: one logical call, one
/// Commit after the source's whole input. Other methods take no source.
pub fn call_streamed(service: &StoreService, call: &SdkCall, source: &mut dyn Source) -> Answer {
    layerfs_persistence::one_call(|| {
        message(&call.args)?;
        match call.method.as_str() {
            "file.write" => {
                let mut one = One {
                    change: Some(args::single(&call.args, "write")),
                    source,
                };
                args::commit_streamed(service, &call.args, &mut one)
            }
            "file.apply" => args::commit_streamed(service, &call.args, source),
            _ => call_once(service, call),
        }
    })
}

/// `file.write`'s one change, its content from the streamed source.
struct One<'s> {
    change: Option<SdkValue>,
    source: &'s mut dyn Source,
}

impl Source for One<'_> {
    fn next(&mut self) -> Result<Option<SdkValue>, layerfs_protocol::sdk::SdkRefusal> {
        match self.change.take() {
            Some(change) => Ok(Some(change)),
            None => self.source.next(),
        }
    }

    fn piece(&mut self) -> Result<Option<&[u8]>, layerfs_protocol::sdk::SdkRefusal> {
        self.source.piece()
    }
}

/// A Commit message is checked before any change.
fn message(args: &SdkValue) -> Result<(), layerfs_protocol::sdk::SdkRefusal> {
    if let Some(value) = args.get("message") {
        if !matches!(value, SdkValue::Null) {
            let SdkValue::Text(message) = value else {
                return Err(args::invalid("message must be text"));
            };
            let mut meta = layerfs_history::CommitMeta::new(layerfs_history::CommitVia::Files);
            meta.message = Some(message.clone());
            meta.check().map_err(|e| args::invalid(e.to_string()))?;
        }
    }
    Ok(())
}

fn call_once(service: &StoreService, call: &SdkCall) -> Answer {
    let args = &call.args;
    message(args)?;
    match call.method.as_str() {
        "store.stats" => store::stats(service),
        "project.stats" => project::stats(service, args),
        "project.hide_path" => project::hide_path(service, args),
        "branch.list" => branch::list(service, args),
        "branch.fork" => branch::fork(service, args),
        "branch.merge" => branch::merge(service, args),
        "branch.publish" => branch::publish(service, args),
        "branch.remove" => branch::remove(service, args),
        "commit.get" => commit::get(service, args),
        "commit.list" => commit::list(service, args),
        "commit.diff" => commit::diff(service, args),
        "file.read" => file::read(service, args),
        "file.history" => file::history(service, args),
        "file.list" => file::list(service, args),
        "file.stat" => file::stat(service, args),
        "file.grep" => file::grep(service, args),
        "file.glob" => file::glob(service, args),
        "file.blame" => file::blame(service, args),
        "file.write" => file::write(service, args),
        "file.edit" => file::edit(service, args),
        "file.mkdir" => file::mkdir(service, args),
        "file.rm" => file::rm(service, args),
        "file.cp" => file::cp(service, args),
        "file.mv" => file::mv(service, args),
        "file.apply" => file::apply(service, args),
        other => Err(crate::lookup::refuse(
            "UnknownMethod",
            format!("no Store method {other}"),
            SdkValue::Null,
        )),
    }
}
