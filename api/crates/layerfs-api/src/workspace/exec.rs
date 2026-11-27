//! `workspace.exec`: run one bash command in a Workspace.
use crate::calls::scope::Rule;
use crate::calls::table::Kind::{Bool, Content, Function, Json, Text};
use crate::calls::table::{a, call, opt, Call, PROGRESS};
use crate::calls::{Context, Form, Policy};
use layerfs_protocol::provision::install::{SdkRefusal, SdkValue};
use layerfs_home::outcome::{refused, Outcome};
use layerfs_workspace::exec::call::{exec, ExecRequest};

const FORM: Form = Form::of(
    &["exit_code"],
    &[
        "stdout",
        "stderr",
        "stdout_file",
        "stderr_file",
        "cancelled",
    ],
    &[
        "exit_code",
        "stdout",
        "stderr",
        "stdout_file",
        "stderr_file",
        "cancelled",
        "exec",
        "stop",
        "stdout_bytes",
        "stderr_bytes",
        "duration_ms",
    ],
);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "workspace",
    "exec",
    &[a("workspace", Text), a("command", Text).redact()],
    &[
        opt("cwd", Text).redact(),
        opt("env", Json).redact().repeat(),
        opt("stdin", Content).redact(),
        opt("stream", Bool),
        opt("stdout", Function),
        opt("stderr", Function),
        PROGRESS,
    ],
    "run one bash command in a Workspace",
    run,
)
.form(&FORM)
.agent()
.scoped(Rule::WorkspaceExec)
.policy(Policy::Workspace("workspace"));

/// `env` as `K=V` entries: one object of names to text (Node, Python and
/// the shell's repeated `--env K=V`).
fn env(value: Option<&SdkValue>) -> Result<Vec<String>, SdkRefusal> {
    let wrong = || refused("InvalidInput", "env is an object of names to text");
    match value {
        None | Some(SdkValue::Null) => Ok(Vec::new()),
        Some(SdkValue::Map(fields)) => (fields.iter())
            .map(|(name, value)| match value {
                SdkValue::Text(text) => Ok(format!("{name}={text}")),
                _ => Err(wrong()),
            })
            .collect(),
        Some(_) => Err(wrong()),
    }
}

/// Runs `workspace.exec`: its side's answer.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let stdin = match args.get("stdin") {
        None | Some(SdkValue::Null) => None,
        Some(SdkValue::Text(text)) => Some(text.clone().into_bytes()),
        Some(SdkValue::Bytes(bytes)) => Some(bytes.clone()),
        Some(_) => return Err(refused("InvalidInput", "stdin is text or bytes")),
    };
    let request = ExecRequest {
        ws: args.get_text("workspace").unwrap_or("").to_owned(),
        command: args.get_text("command").map(str::to_owned),
        cwd: args.get_text("cwd").map(str::to_owned),
        env: env(args.get("env"))?,
        stdin,
        tag: context.tag.clone(),
    };
    let (home, events) = context.parts();
    exec(home, events, &request)
}
