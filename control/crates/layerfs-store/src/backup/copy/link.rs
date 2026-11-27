//! One Store's link to a bucket, which the Store's record holds under
//! `backups.<bucket>`.
use crate::admin::status::get::hex_text;
use crate::backup::bucket::pieces;
use layerfs_home::home;
use layerfs_protocol::sdk::hex;
use layerfs_protocol::sdk::SdkValue;

/// One Store's link to a bucket, its record's `backups.<bucket>`: the file
/// it backs up to, the heads it had and what its last transfer moved.
#[derive(Clone, Debug, PartialEq)]
pub struct StoreLink {
    fields: SdkValue,
}

impl StoreLink {
    /// A link `value` as a Store record holds it.
    pub fn from_record(value: SdkValue) -> Self {
        Self { fields: value }
    }

    /// The link a transfer `op` (`create` or `restore`) of file `file` leaves.
    pub fn new(
        file: &str,
        op: &str,
        bytes: u64,
        sha256: &[u8; 32],
        heads: Option<&[u8; 32]>,
    ) -> Self {
        Self::from_record(SdkValue::map([
            ("file", SdkValue::text(file)),
            (
                "heads",
                heads.map(|heads| hex_text(heads)).unwrap_or_default(),
            ),
            (
                "last",
                SdkValue::map([
                    ("op", SdkValue::text(op)),
                    ("at", SdkValue::Int(home::now_ms())),
                    ("bytes", SdkValue::Int(bytes as i64)),
                    ("sha256", hex_text(sha256)),
                ]),
            ),
        ]))
    }

    /// The bucket file it backs up to.
    pub fn file(&self) -> Option<&str> {
        self.fields.get_text("file")
    }

    /// The heads digest it records, as hex.
    pub fn heads(&self) -> Option<String> {
        match self.fields.get("heads") {
            Some(SdkValue::Text(text)) => Some(text.clone()),
            Some(SdkValue::Bytes(bytes)) => Some(hex(bytes)),
            _ => None,
        }
    }

    /// The sha256 of the copy its last transfer moved.
    pub fn sha256(&self) -> Option<[u8; 32]> {
        self.fields.get("last").and_then(|l| match l.get("sha256") {
            Some(SdkValue::Text(t)) => pieces::parse_hex32(t),
            Some(SdkValue::Bytes(b)) => b.as_slice().try_into().ok(),
            _ => None,
        })
    }

    /// The link as written.
    pub fn to_record(&self) -> &SdkValue {
        &self.fields
    }

    /// Whether a transfer through it ever worked (it records a `last`): a
    /// link written only for a failed first attempt has none.
    pub fn transferred(&self) -> bool {
        matches!(self.fields.get("last"), Some(SdkValue::Map(_)))
    }

    /// This link with `attempt` recording a transfer `call` (`create` or
    /// `restore`) that did not work, and why: `{call, at, error}`. A link
    /// that did not exist yet names `file` and nothing else.
    pub fn with_attempt(link: Option<Self>, file: &str, call: &str, error: &str) -> Self {
        let fields = link.map_or_else(
            || SdkValue::map([("file", SdkValue::text(file))]),
            |link| link.fields,
        );
        let attempt = SdkValue::map([
            ("call", SdkValue::text(call)),
            ("at", SdkValue::Int(home::now_ms())),
            ("error", SdkValue::text(error)),
        ]);
        Self::from_record(home::with_field(
            fields,
            crate::admin::status::get::ATTEMPT,
            attempt,
        ))
    }
}
