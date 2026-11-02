//! `layerfs`: the library's call table as a shell (S §2.1, #533 step 15).
//!
//! `layerfs <family> <call> <positionals> --options` makes exactly the
//! library's one call, once; the result is one line of JSON on stdout, a
//! refusal is JSON on stderr. Exit 0: the call succeeded; 2: the command
//! line is wrong and nothing was called; 125: LayerFS refused or failed the
//! call. `workspace exec --stream` exits with the command's own code.
//! `layerfs worker` serves a machine's worker; `layerfs help` and
//! `layerfs --version` need no home.
#![forbid(unsafe_code)]

mod help;
mod output;
mod parse;
mod run;
mod worker;

fn main() -> std::process::ExitCode {
    run::main(std::env::args().skip(1).collect())
}
