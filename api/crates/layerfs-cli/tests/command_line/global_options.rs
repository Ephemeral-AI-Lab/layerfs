//! The call surface's global options on the command line (2.0): `--detail`
//! and `--with a,b` are words every call takes and the library reads; a
//! secret option is read from stdin through its own flag and no other way;
//! a refusal is one JSON line on stderr in the form asked. Against the real
//! binary for the three migrated calls, and against the parser with a
//! made-up entry for the secret, which no call of the table declares yet.
use crate::common;
use common::{layerfs, run, run_with_stdin, tempdir, text};
use layerfs_api::dispatch::table::{a, call, opt};
use layerfs_api::{json, Call, Context, Kind, Section};
use layerfs_cli::parse::parse;
use layerfs_protocol::sdk::SdkValue;

fn never(_: &mut Context<'_>, _: &SdkValue) -> Result<SdkValue, layerfs_protocol::sdk::SdkRefusal> {
    unreachable!("the parser calls nothing")
}

const SECRET: Call = call(
    "made",
    "up",
    &[a("name", Kind::Name)],
    &[
        opt("user", Kind::Name),
        opt("password", Kind::Content)
            .redact()
            .from_stdin("--password-stdin"),
    ],
    "a made-up call with a secret",
    never,
)
.with(&[Section::own("live", &["observed_at"])]);

fn words(line: &[&str]) -> Vec<String> {
    line.iter().map(|word| (*word).to_owned()).collect()
}

#[test]
fn a_secret_is_read_from_stdin_through_its_flag_and_no_other_way() {
    let line = words(&["db", "--user", "me", "--password-stdin"]);
    let parsed =
        parse(&SECRET, &line, &mut &b"s3cr3t pass\n"[..]).unwrap_or_else(|u| panic!("{}", u.0));
    assert_eq!(parsed.args.get_text("name"), Some("db"));
    assert_eq!(parsed.args.get_text("user"), Some("me"));
    // all of stdin, less one trailing newline
    assert_eq!(parsed.args.get_text("password"), Some("s3cr3t pass"));
    let parsed =
        parse(&SECRET, &line, &mut &b"two\nlines\n\n"[..]).unwrap_or_else(|u| panic!("{}", u.0));
    assert_eq!(parsed.args.get_text("password"), Some("two\nlines\n"));
    // not given: the option is simply absent, and stdin is not read
    let parsed =
        parse(&SECRET, &words(&["db"]), &mut &b"unread"[..]).unwrap_or_else(|u| panic!("{}", u.0));
    assert!(parsed.args.get("password").is_none());
    // the value never rides the command line
    for line in [
        &["db", "--password", "s3cr3t"][..],
        &["db", "--password=s3cr3t"],
        &["db", "--password-stdin=s3cr3t"],
    ] {
        let refused = parse(&SECRET, &words(line), &mut &b""[..]).err().unwrap().0;
        assert!(refused.contains("stdin"), "{line:?}: {refused}");
        assert!(!refused.contains("s3cr3t"), "{line:?}: {refused}");
    }
    // an unknown option lists the flag, not the option's name
    let refused = parse(&SECRET, &words(&["db", "--bogus"]), &mut &b""[..])
        .err()
        .unwrap()
        .0;
    assert!(
        refused.contains("--password-stdin") && !refused.contains("--password,"),
        "{refused}"
    );
    assert!(
        refused.contains("--detail") && refused.contains("--with"),
        "{refused}"
    );
}

#[test]
fn detail_and_with_are_passed_to_the_library_as_they_were_written() {
    let line = words(&["db", "--detail", "--with", "live, live"]);
    let parsed = parse(&SECRET, &line, &mut &b""[..]).unwrap_or_else(|u| panic!("{}", u.0));
    assert!(parsed.switches.detail);
    assert_eq!(parsed.args.get("detail"), Some(&SdkValue::Bool(true)));
    assert_eq!(parsed.args.get_text("with"), Some("live, live"));
    // a section the entry does not declare is a wrong command line
    let line = words(&["db", "--with", "live,stats"]);
    let refused = parse(&SECRET, &line, &mut &b""[..]).err().unwrap().0;
    assert_eq!(
        refused,
        "made.up: --with names `stats`; this call takes live"
    );
    let parsed = parse(&SECRET, &words(&["db", "--with=live"]), &mut &b""[..])
        .unwrap_or_else(|u| panic!("{}", u.0));
    assert!(!parsed.switches.detail);
    assert_eq!(parsed.args.get_text("with"), Some("live"));
    let refused = parse(&SECRET, &words(&["db", "--with"]), &mut &b""[..])
        .err()
        .unwrap()
        .0;
    assert!(refused.contains("--with needs a value"), "{refused}");
    let refused = parse(&SECRET, &words(&["db", "--detail=yes"]), &mut &b""[..])
        .err()
        .unwrap()
        .0;
    assert!(refused.contains("unknown option"), "{refused}");
}

fn out(home: &std::path::Path, line: &[&str]) -> (Option<i32>, String, String) {
    let out = run(&mut layerfs(home, line));
    (out.status.code(), text(&out.stdout), text(&out.stderr))
}

#[test]
fn home_get_prints_the_form_asked_and_a_refusal_is_one_line_on_stderr() {
    let home = tempdir("forms");
    std::fs::remove_dir_all(&home).unwrap();
    // no home yet: the short form says what is wrong and the fix
    let (code, stdout, stderr) = out(&home, &["home", "get"]);
    assert_eq!((code, stderr.as_str()), (Some(0), ""));
    let short = json::parse(stdout.trim()).unwrap();
    assert_eq!(short.get_text("state"), Some("broken"));
    assert_eq!(short.get_text("reason"), Some("missing"));
    assert_eq!(short.get_text("fix"), Some("layerfs home init"));
    assert!(short.get("layout").is_none() && short.get("bundle").is_none());
    assert!(!home.exists());
    assert_eq!(out(&home, &["home", "init"]).0, Some(0));
    let (code, stdout, _) = out(&home, &["home", "get", "--detail"]);
    assert_eq!(code, Some(0));
    let long = json::parse(stdout.trim()).unwrap();
    assert_eq!(long.get_text("state"), Some("ready"));
    assert_eq!(long.get("reason"), Some(&SdkValue::Null));
    assert!(long.get("bundle").is_some() && long.get("tree").is_none());
    // sections, as one comma-separated word
    let (code, stdout, stderr) = out(&home, &["home", "get", "--with", "platforms,tree"]);
    assert_eq!(code, Some(0), "{stderr}");
    let both = json::parse(stdout.trim()).unwrap();
    let platforms = both.get("platforms").unwrap();
    assert!(platforms.get_text("host").is_some());
    assert!(both.get("tree").unwrap().get("machines").is_some());
    // a section the call does not take: the table says so, so the shell
    // refuses the line (exit 2) and nothing is called
    let (code, stdout, stderr) = out(&home, &["home", "get", "--with", "live"]);
    assert_eq!((code, stdout.as_str()), (Some(2), ""));
    assert!(
        stderr.contains("home.get: --with names `live`; this call takes platforms, tree"),
        "{stderr}"
    );
    // a library refusal: exit 125, one JSON line on stderr, nothing on
    // stdout; its long form with --detail
    let (code, stdout, stderr) = out(&home, &["home", "log", "--limit", "0"]);
    assert_eq!((code, stdout.as_str()), (Some(125), ""));
    assert!(
        stderr.starts_with("{\"error\":\"InvalidInput\",\"message\":\"home.log: `limit` is 1 to ")
            && stderr.ends_with("\"}\n")
            && stderr.lines().count() == 1,
        "{stderr}"
    );
    let (code, _, stderr) = out(&home, &["home", "log", "--limit", "0", "--detail"]);
    assert_eq!(code, Some(125));
    assert!(
        stderr.ends_with("\"fix\":null,\"detail\":null}\n"),
        "{stderr}"
    );
    // a wrong command line is exit 2 and not JSON
    let (code, stdout, stderr) = out(&home, &["home", "get", "--bogus"]);
    assert_eq!((code, stdout.as_str()), (Some(2), ""));
    assert!(stderr.starts_with("layerfs: "), "{stderr}");
    // help names the global options and each call's sections
    let (code, help, _) = out(&home, &["help", "home", "get"]);
    assert_eq!(code, Some(0));
    assert!(help.contains("[--with platforms,tree]"), "{help}");
    let (_, help, _) = out(&home, &["help"]);
    assert!(
        help.contains("--detail") && help.contains("--with"),
        "{help}"
    );
    assert!(help.contains("--text, --json"), "{help}");
    // --text still renders a migrated call's result
    let piped = run_with_stdin(&mut layerfs(&home, &["home", "get", "--text"]), b"");
    assert_eq!(piped.status.code(), Some(0), "{}", text(&piped.stderr));
    assert!(
        text(&piped.stdout).contains("ready"),
        "{}",
        text(&piped.stdout)
    );
    std::fs::remove_dir_all(home).unwrap();
}
