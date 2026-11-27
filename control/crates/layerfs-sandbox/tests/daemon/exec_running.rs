//! What an exec record says of its command for `sandbox.get`'s `running`
//! (layerfs-project#178): the log tag of the call that started it and the
//! program it starts with, never an argument or an environment value; a
//! record from before those fields still decodes; and `running` lists the
//! commands whose owner process still runs, oldest first, leaving a stale
//! record out and untouched.
use layerfs_home::home::write_entity;
use layerfs_protocol::sdk::SdkValue;
use layerfs_sandbox::exec::id::ExecId;
use layerfs_sandbox::exec::settle::unreached;
use layerfs_sandbox::record::exec::{self, program, running, ExecRecord, Route, Running};
use std::path::PathBuf;

fn folder(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-exec-running-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_program_is_the_first_word_never_an_argument_or_an_environment_value() {
    let long = "p".repeat(80);
    for (command, expected) in [
        ("npm test", Some("npm")),
        ("  sleep 20", Some("sleep")),
        ("\tls\n-l", Some("ls")),
        ("CI=1 npm run build", Some("npm")),
        ("TOKEN=s3cret NODE_ENV=test npx vitest", Some("npx")),
        // a quoted value stays one word: none of it is taken for the program
        ("TOKEN=\"a b c\" ./run.sh --flag", Some("./run.sh")),
        ("A='x y' B=2 env C=3 node s.js", Some("env")),
        ("KEY=a\\ b make", Some("make")),
        ("\"/opt/my tool/bin\" arg", Some("\"/opt/my tool/bin\"")),
        ("cd /work && npm test", Some("cd")),
        // not an assignment as bash reads it
        ("1A=2 x", Some("1A=2")),
        ("a-b=c x", Some("a-b=c")),
        ("ls\u{1b}[31m x", Some("ls[31m")),
        ("", None),
        ("   ", None),
        ("ONLY=assignments HERE=1", None),
        (long.as_str(), Some(&long[..64])),
    ] {
        assert_eq!(program(command).as_deref(), expected, "{command:?}");
    }
}

#[test]
fn a_record_keeps_its_tag_and_program_and_one_from_before_them_decodes() {
    let id = ExecId::parse("e1").unwrap();
    let made = ExecRecord::new(&id, "ab12", ("/w/1", Some("w1"))).started_by(
        Some("dsh:session-7:turn-2"),
        "TOKEN=zq9 npm run build -- --secret=1",
    );
    assert_eq!(made.tag.as_deref(), Some("dsh:session-7:turn-2"));
    assert_eq!(made.program.as_deref(), Some("npm"));
    let written = made.encode();
    assert_eq!(written.get_text("tag"), Some("dsh:session-7:turn-2"));
    assert_eq!(written.get_text("program"), Some("npm"));
    assert_eq!(ExecRecord::decode(&written), Some(made.clone()));
    // nothing of the command but its program is in the record
    let text = format!("{written:?}");
    for kept_out in ["zq9", "secret", "build", "TOKEN"] {
        assert!(!text.contains(kept_out), "{kept_out}: {text}");
    }
    assert_eq!(
        made.shown(),
        SdkValue::map([
            ("exec_id", SdkValue::text("e1")),
            ("started", SdkValue::Int(made.started)),
            ("tag", SdkValue::text("dsh:session-7:turn-2")),
            ("program", SdkValue::text("npm")),
        ])
    );

    // a tag a log line would not carry is not kept
    let plain = || ExecRecord::new(&id, "ab12", ("/w/1", None));
    for unusable in ["", "a\nb", &"t".repeat(129)] {
        assert_eq!(
            plain().started_by(Some(unusable), "ls").tag,
            None,
            "{unusable:?}"
        );
    }
    assert_eq!(plain().started_by(None, "ls").tag, None);

    // a record written before the fields has neither, and it decodes; a
    // rewrite (a cancel's mark, a stale claim put back) adds neither
    let before = plain().encode();
    assert!(before.get("tag").is_none() && before.get("program").is_none());
    let old = ExecRecord::decode(&before).expect("an earlier record decodes");
    assert_eq!((old.tag.as_deref(), old.program.as_deref()), (None, None));
    assert_eq!(old.encode(), before);
    assert_eq!(old.shown().get("tag"), Some(&SdkValue::Null));
    assert_eq!(old.shown().get("program"), Some(&SdkValue::Null));
    assert_eq!(old.shown().get_text("exec_id"), Some("e1"));
}

#[test]
fn running_lists_the_commands_whose_owner_still_runs() {
    let dir = folder("live");
    assert_eq!(running(&dir), Some(Vec::new()), "no record folder yet");

    // two commands this process owns, in two Workspaces, and their order
    let start = |id: &str, workspace: &str, started: i64, route: Route| {
        let id = ExecId::parse(id).unwrap();
        let mut made =
            ExecRecord::new(&id, workspace, ("/w", None)).started_by(Some("t"), "sleep 20");
        made.started = started;
        made.route = route;
        Running::start(&dir, "s1", &made, &mut unreached).unwrap()
    };
    let later = start("e-later", "aa", 2_000, Route::Local);
    let first = start("e-first", "bb", 1_000, Route::Machine("lima".into()));

    // stale records: an owner that never was, and this process's id with
    // another start time (a reused id)
    let stale = |id: &str, owner: SdkValue| {
        let id = ExecId::parse(id).unwrap();
        let mut made = ExecRecord::new(&id, "aa", ("/w", None)).started_by(None, "npm test");
        made.owner = owner;
        let file = exec::file(&dir, "aa", id.as_str());
        write_entity(&file, &made.encode()).unwrap();
        file
    };
    let gone = stale("e-gone", SdkValue::map([("pid", SdkValue::Int(0))]));
    let reused = stale(
        "e-reused",
        SdkValue::map([
            ("pid", SdkValue::Int(i64::from(std::process::id()))),
            ("pid_start", SdkValue::text("1")),
            ("pid_boot", SdkValue::text(format!("{:032x}", 1))),
        ]),
    );

    let ids = |dir: &PathBuf| -> Vec<(String, String)> {
        (running(dir).unwrap().into_iter())
            .map(|record| (record.id, record.workspace))
            .collect()
    };
    let pair = |id: &str, workspace: &str| (id.to_owned(), workspace.to_owned());
    assert_eq!(ids(&dir), [pair("e-first", "bb"), pair("e-later", "aa")]);
    // a read: the stale records are still there, as they were
    assert!(gone.exists() && reused.exists());

    // a command that ended is listed no more
    first.end().unwrap();
    assert_eq!(ids(&dir), [pair("e-later", "aa")]);
    drop(later);
    assert_eq!(ids(&dir), []);
    assert!(gone.exists() && reused.exists());

    // records that cannot be read: not an empty list
    std::fs::write(exec::file(&dir, "aa", "e-broken"), b"{").unwrap();
    assert_eq!(running(&dir), None);
    std::fs::remove_dir_all(&dir).unwrap();
}
