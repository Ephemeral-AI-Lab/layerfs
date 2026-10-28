//! The public method table: each JavaScript method name runs its method file
//! once. Operator names are absent from this table.
use crate::base::args::{self, Answer};
use crate::{branch, fs, project};
use layerfs_bridge::control::{SdkCall, SdkValue};
use layerfs_store_service::StoreService;

/// Runs one public Store method once, in this process, on `service`.
pub fn call(service: &StoreService, call: &SdkCall) -> Answer {
    let args = &call.args;
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
    match call.method.as_str() {
        "project.list_branches" => project::list_branches(service, args),
        "branch.fork" => branch::fork(service, args),
        "branch.list_commits" => branch::list_commits(service, args),
        "branch.merge" => branch::merge(service, args),
        "branch.publish" => branch::publish(service, args),
        "branch.diff_commits" => branch::diff_commits(service, args),
        "branch.diff_branches" => branch::diff_branches(service, args),
        "fs.read" => fs::read(service, args),
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
        other => Err(layerfs_store_service::history::lookup::refuse(
            "UnknownMethod",
            format!("no Store method {other}"),
            SdkValue::Null,
        )),
    }
}
