//! Names of machines, Stores, buckets, projects and Sandboxes (S §2.3): 1 to
//! 63 lowercase letters, digits, `_`, `.` and `-`, starting and ending with a
//! letter or digit, never a Windows device name (each name is a folder in
//! the home). A project is always `<store>/<project>`.

/// Windows device names, refused everywhere.
const DEVICES: [&str; 22] = [
    "con", "nul", "aux", "prn", "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8",
    "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// The built-in machines' names, which `machine create` and `connect` and
/// `sandbox create` refuse as new names.
pub const BUILT_IN: [&str; 2] = ["native", "docker"];

/// `name` as one machine, Store, bucket, project or Sandbox name, or why not.
pub fn name(what: &str, name: &str) -> Result<(), String> {
    let bytes = name.as_bytes();
    let edge = |b: &u8| b.is_ascii_lowercase() || b.is_ascii_digit();
    let valid = (1..=63).contains(&bytes.len())
        && bytes
            .iter()
            .all(|b| edge(b) || matches!(b, b'_' | b'.' | b'-'))
        && bytes.first().is_some_and(edge)
        && bytes.last().is_some_and(edge);
    if !valid {
        return Err(format!(
            "{what} name `{name}` must be 1-63 lowercase letters, digits, _ . -, starting and ending with a letter or digit"
        ));
    }
    if DEVICES.contains(&name) {
        return Err(format!("{what} name `{name}` is reserved"));
    }
    Ok(())
}

/// `<store>/<project>` split into its two names, or why not. A bare project
/// name is refused even when one Store holds it (H §7.5).
pub fn project(text: &str) -> Result<(&str, &str), String> {
    let Some((store, project)) = text.split_once('/') else {
        return Err(format!(
            "project `{text}` must be written with its Store, as <store>/<project>"
        ));
    };
    name("Store", store)?;
    name("project", project)?;
    Ok((store, project))
}
