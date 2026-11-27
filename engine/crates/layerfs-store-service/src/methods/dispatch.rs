//! The public method table: each JavaScript method name runs its method file
//! once. Operator names are absent from this table.
use crate::methods::args::{self, Answer};
use crate::methods::{branch, file as fs, project, store};
use crate::write::files::Source;
use crate::StoreService;
use layerfs_protocol::sdk::{SdkCall, SdkValue};

/// Runs one public Store method once, in this process, on `service`: one
/// call, so a quarantined PostgreSQL session reopens at most once in it.
pub fn call(service: &StoreService, call: &SdkCall) -> Answer {
    layerfs_persistence::one_call(|| call_once(service, call))
}

/// Runs `fs.write` or `fs.apply` once on `service` with its content or its
/// changes pulled from `source` as they are applied: one logical call, one
/// Commit after the source's whole input. Other methods take no source.
pub fn call_streamed(service: &StoreService, call: &SdkCall, source: &mut dyn Source) -> Answer {
    layerfs_persistence::one_call(|| {
        message(&call.args)?;
        match call.method.as_str() {
            "fs.write" => {
                let mut one = One {
                    change: Some(args::single(&call.args, "write")),
                    source,
                };
                args::commit_streamed(service, &call.args, &mut one)
            }
            "fs.apply" => args::commit_streamed(service, &call.args, source),
            _ => call_once(service, call),
        }
    })
}

/// `fs.write`'s one change, its content from the streamed source.
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
        "project.list_branches" => project::list_branches(service, args),
        "project.remove_file" => project::remove_file(service, args),
        "branch.fork" => branch::fork(service, args),
        "branch.list_commits" => branch::list_commits(service, args),
        "branch.get_commit" => branch::get_commit(service, args),
        "branch.merge" => branch::merge(service, args),
        "branch.publish" => branch::publish(service, args),
        "branch.remove" => branch::remove(service, args),
        "branch.diff_commits" => branch::diff_commits(service, args),
        "branch.diff_branches" => branch::diff_branches(service, args),
        "commit.diff" => branch::diff(service, args),
        "fs.read" => fs::read(service, args),
        "fs.revision" => fs::revision(service, args),
        "fs.list" => fs::list(service, args),
        "fs.stat" => fs::stat(service, args),
        "fs.grep" => fs::grep(service, args),
        "fs.glob" => fs::glob(service, args),
        "fs.diff_files" => fs::diff_files(service, args),
        "fs.blame" => fs::blame(service, args),
        "fs.write" => fs::write(service, args),
        "fs.edit" => fs::edit(service, args),
        "fs.mkdir" => fs::mkdir(service, args),
        "fs.rm" => fs::rm(service, args),
        "fs.cp" => fs::cp(service, args),
        "fs.mv" => fs::mv(service, args),
        "fs.apply" => fs::apply(service, args),
        other => Err(crate::history::lookup::refuse(
            "UnknownMethod",
            format!("no Store method {other}"),
            SdkValue::Null,
        )),
    }
}
