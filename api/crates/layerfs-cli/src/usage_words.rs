//! The value words of the usage lines (call surface 2.0 `cli`, conformance
//! R2 G1): an option shows the default its call's row shows (`[--limit
//! 100]`), else the one word every row gives it (`[--at C]`), else its own
//! name (`[--title <title>]`). A test compares every usage line with its
//! row; the defaults themselves are the calls' own.

/// The defaults the rows show, by call and option.
const DEFAULTS: &[(&str, &str, &str)] = &[
    ("project.init", "timeout", "3600000"),
    ("project.list", "limit", "100"),
    ("file.read", "line", "1"),
    ("file.read", "limit", "2000"),
    ("file.list", "limit", "100"),
    ("file.glob", "limit", "100"),
    ("file.grep", "limit", "100"),
    ("file.history", "traversal", "provenance"),
    ("file.history", "limit", "100"),
    ("file.blame", "line", "1"),
    ("file.blame", "limit", "2000"),
    ("sandbox.list", "limit", "100"),
    ("sandbox.files", "depth", "1"),
    ("sandbox.files", "limit", "100"),
    ("sandbox.files", "byte_offset", "0"),
    ("sandbox.files", "byte_length", "262144"),
    ("sandbox.log", "limit", "200"),
    ("workspace.list", "limit", "100"),
    ("branch.list", "limit", "100"),
    ("commit.list", "limit", "100"),
    ("commit.diff", "limit", "100"),
    ("home.log", "limit", "200"),
    ("machine.files", "depth", "1"),
    ("machine.files", "limit", "100"),
    ("machine.files", "byte_offset", "0"),
    ("machine.files", "byte_length", "262144"),
    ("machine.log", "limit", "200"),
    ("machine.create", "cpus", "2"),
    ("machine.create", "memory", "4G"),
    ("machine.create", "disk", "20G"),
    ("store.connect", "synchronous_commit", "local"),
    ("store.repair_mount_holders", "limit", "100"),
];

/// The value word of each option no row shows a default for.
const WORDS: &[(&str, &str)] = &[
    ("access_key", "K"),
    ("at", "C"),
    ("branch", "B"),
    ("bucket", "B"),
    ("call", "C"),
    ("client", "C"),
    ("cpus", "N"),
    ("cursor", "C"),
    ("cwd", "D"),
    ("daemon", "P"),
    ("docker", "S"),
    ("exclude", "G"),
    ("exclude_prefix", "P"),
    ("exec", "E"),
    ("expected", "C"),
    ("file", "F"),
    ("from", "R"),
    ("image", "I"),
    ("include", "G"),
    ("into", "NEW"),
    ("machine", "M"),
    ("memory", "M"),
    ("mode", "M"),
    ("mount", "B"),
    ("name", "N"),
    ("new_string", "S"),
    ("old_string", "S"),
    ("op", "HEX"),
    ("operator", "NAME"),
    ("os_image", "I"),
    ("outcome", "O"),
    ("path", "P"),
    ("prefix", "P"),
    ("project", "P"),
    ("sandbox", "S"),
    ("since", "T"),
    ("store", "S"),
    ("synchronous_commit", "S"),
    ("tag", "T"),
    ("units", "JSON"),
    ("user", "U"),
    ("wsl", "D"),
];

/// The value word option `option` of call `call` (`family.call`) shows.
pub fn word(call: &str, option: &str) -> String {
    let default = DEFAULTS
        .iter()
        .find(|(of, name, _)| *of == call && *name == option);
    let word = WORDS.iter().find(|(name, _)| *name == option);
    match (default, word) {
        (Some((_, _, default)), _) => (*default).to_owned(),
        (None, Some((_, word))) => (*word).to_owned(),
        (None, None) => format!("<{}>", option.replace('_', "-")),
    }
}
