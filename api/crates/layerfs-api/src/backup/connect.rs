//! `backup.connect`: connect a bucket (r2, or an S3 endpoint) by name; its
//! key's secret comes on stdin in the shell and is kept in the home's
//! secret folder, never answered.
use crate::calls::table::Kind::{Content, Name, Text};
use crate::calls::table::{a, call, opt, Call};
use crate::calls::{Context, Form};
use layerfs_bridge::control::SdkValue;
use layerfs_home::names::BucketName;
use layerfs_home::outcome::{refused, Outcome};
use layerfs_store::backup::family::connect;
use layerfs_store::backup::record::BucketConnect;

/// Its answer: `ok`; the long form names the bucket, its provider, URL
/// and key id.
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
        opt("access_key", Text),
        opt("secret_key", Content).from_stdin("--key-stdin"),
    ],
    "connect a bucket (r2, or an S3 endpoint) for Store backups",
    run,
)
.form(&FORM);

/// Option `name` as text; both key options are required.
fn required(args: &SdkValue, name: &str) -> Result<String, layerfs_bridge::control::SdkRefusal> {
    let value = match args.get(name) {
        Some(SdkValue::Text(text)) => Some(text.clone()),
        Some(SdkValue::Bytes(bytes)) => Some(
            String::from_utf8(bytes.clone())
                .map_err(|_| refused("InvalidInput", format!("`{name}` must be UTF-8 text")))?,
        ),
        _ => None,
    };
    value.filter(|text| !text.is_empty()).ok_or_else(|| {
        refused(
            "InvalidInput",
            format!("backup.connect: `{name}` is required"),
        )
    })
}

/// Runs `backup.connect`: one listing with the key before anything is
/// recorded.
fn run(context: &mut Context<'_>, args: &SdkValue) -> Outcome {
    let request = BucketConnect {
        name: BucketName::parse(args.get_text("name").unwrap_or(""))?,
        url: args.get_text("url").unwrap_or_default().to_owned(),
        access_key: required(args, "access_key")?,
        secret_key: required(args, "secret_key")?,
    };
    connect(&context.home, &request)
}
