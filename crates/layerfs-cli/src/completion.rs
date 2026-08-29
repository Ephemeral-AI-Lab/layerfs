#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Completion {
    pub value: String,
    pub description: String,
}

pub(crate) fn complete(input: &str, cursor: usize) -> Vec<Completion> {
    let prefix = &input[..cursor.min(input.len())];
    let candidates: &[(&str, &str)] = if !prefix.contains(' ') {
        &[
            ("db", "database connections"),
            ("layer", "Layer authority"),
            ("stack", "Stack construction"),
            ("branch", "Branch work"),
            ("workspace", "Workspace sessions"),
            ("monitor", "observation"),
        ]
    } else {
        &[]
    };
    candidates
        .iter()
        .filter(|(value, _)| value.starts_with(prefix.trim()))
        .map(|(value, description)| Completion {
            value: (*value).to_owned(),
            description: (*description).to_owned(),
        })
        .collect()
}
