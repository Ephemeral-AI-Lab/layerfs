//! Owned scalar bindings for history statements.
use crate::backend::seam::Param;
use layerfs_history::records::GcIntent;
pub(crate) trait Parameters {
    fn into_params(self) -> Vec<Param>;
}
impl Parameters for Vec<Param> {
    fn into_params(self) -> Vec<Param> {
        self
    }
}
impl<T: Into<Param>, const N: usize> Parameters for [T; N] {
    fn into_params(self) -> Vec<Param> {
        self.into_iter().map(Into::into).collect()
    }
}
macro_rules! params { ($($value:expr),* $(,)?) => { vec![$(crate::backend::seam::Param::from($value)),*] }; }
pub(crate) use params;
impl From<&[u8]> for Param {
    fn from(v: &[u8]) -> Self {
        Self::Bytes(v.to_vec())
    }
}
impl From<Vec<u8>> for Param {
    fn from(v: Vec<u8>) -> Self {
        Self::Bytes(v)
    }
}
impl From<Option<Vec<u8>>> for Param {
    fn from(v: Option<Vec<u8>>) -> Self {
        Self::OptionalBytes(v)
    }
}
impl From<Option<String>> for Param {
    fn from(v: Option<String>) -> Self {
        Self::OptionalText(v)
    }
}
impl From<&str> for Param {
    fn from(v: &str) -> Self {
        Self::Text(v.to_owned())
    }
}
impl From<String> for Param {
    fn from(v: String) -> Self {
        Self::Text(v)
    }
}
impl From<i64> for Param {
    fn from(v: i64) -> Self {
        Self::I64(v)
    }
}

/// The prefix of a live `gc_intent` row's place tokens in its `evidence`
/// column. A retained `done` row's audit text never carries it.
const PLACE_EVIDENCE: &str = "place:v1";

/// Encodes a live intent's place tokens into `evidence`:
/// `place:v1;inc=<incarnation>;gen=<generation>`, an absent part omitted and
/// NULL when both are absent. A token that is empty or holds `;` cannot round
/// trip and is not recorded: missing evidence is no proof, never death.
pub(crate) fn place_evidence(intent: &GcIntent) -> Option<String> {
    let mut text = String::from(PLACE_EVIDENCE);
    let tokens = [&intent.machine_incarnation, &intent.sandbox_generation];
    for (key, token) in ["inc", "gen"].into_iter().zip(tokens.map(Option::as_deref)) {
        if let Some(token) = token.filter(|t| !t.is_empty() && !t.contains(';')) {
            text.push_str(&format!(";{key}={token}"));
        }
    }
    (text.len() > PLACE_EVIDENCE.len()).then_some(text)
}

/// Decodes [`place_evidence`] into `(machine_incarnation, sandbox_generation)`.
/// NULL, a value without the `place:v1` prefix (a `done` row's audit text)
/// or a malformed value yields no tokens.
pub(crate) fn place_tokens(evidence: Option<String>) -> (Option<String>, Option<String>) {
    let text = evidence.unwrap_or_default();
    let Some(rest) = text
        .strip_prefix(PLACE_EVIDENCE)
        .and_then(|r| r.strip_prefix(';'))
    else {
        return (None, None);
    };
    let (mut incarnation, mut generation) = (None, None);
    for part in rest.split(';') {
        let (slot, token) = match part.split_once('=') {
            Some(("inc", token)) if incarnation.is_none() => (&mut incarnation, token),
            Some(("gen", token)) if generation.is_none() => (&mut generation, token),
            _ => return (None, None),
        };
        if token.is_empty() {
            return (None, None);
        }
        *slot = Some(token.to_owned());
    }
    (incarnation, generation)
}
