//! `backup.connect`: connect a bucket (r2, or an S3 endpoint) by name; its
//! key's secret comes on stdin in the shell and is kept in the home's
//! secret folder, never answered. Connected again at the same URL it
//! replaces the key and keeps the Stores' links (spec v21).
use crate::calls::table::Kind::{Content, Name, Text};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::{Context, Form};
use layerfs_home::names::BucketName;
use layerfs_home::outcome::{refused, Outcome};
use layerfs_protocol::sdk::SdkValue;
use layerfs_store::backup::family::connect;
use layerfs_store::backup::record::BucketConnect;

/// Its answer: `ok`; the long form says `connected` or `rotated` and names
/// the bucket, its provider, URL and key id.
const FORM: Form = Form::of(
    &["ok"],
    &[],
    &["ok", "outcome", "bucket", "provider", "url", "access_key"],
);

/// Its table entry.
pub(crate) const CALL: Call = call(
    "backup",
    "connect",
    &[a("name", Name), a("url", Text).redact()],
    &[
        opt("access_key", Text).required(),
        opt("secret_key", Content)
            .from_stdin("--key-stdin")
            .required(),
    ],
    "connect a bucket (r2, or an S3 endpoint) for Store backups",
    run,
)
.form(&FORM);

/// Option `name` as text: bytes must be UTF-8, and it may not be empty.
/// The table requires both key options, so a missing one is refused
/// before the call runs.
fn text(args: &SdkValue, name: &str) -> Result<String, layerfs_protocol::sdk::SdkRefusal> {
    let text = match args.get(name) {
        Some(SdkValue::Bytes(bytes)) => String::from_utf8(bytes.clone())
            .map_err(|_| refused("InvalidInput", format!("`{name}` must be UTF-8 text")))?,
        _ => args.get_text(name).unwrap_or_default().to_owned(),
    };
    match text.is_empty() {
        true => Err(refused(
            "InvalidInput",
            format!("backup.connect: `{name}` is empty"),
        )),
        false => Ok(text),
    }
}

/// Runs `backup.connect`: one listing with the key before anything is
/// recorded or replaced.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let request = BucketConnect {
        name: BucketName::parse(args.get_text("name").unwrap_or(""))?,
        url: args.get_text("url").unwrap_or_default().to_owned(),
        access_key: text(args, "access_key")?,
        secret_key: text(args, "secret_key")?,
    };
    connect(&context.home, &request)
}
