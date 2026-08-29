use layerfs_cli::{CliSession, Command, WorkspaceCommand};

#[test]
fn frozen_grammar_parses_every_command_group_without_aliases() {
    for line in [
        "db create layer /tmp/layer.db",
        "db connect stack /tmp/stack.db",
        "db use /tmp/stack.db",
        "db disconnect /tmp/stack.db",
        "db list",
        "layer init /tmp/root",
        "layer init --empty",
        "layer pull 320000000000000000000000000000000000000000000000000000000000000000",
        "layer add --from 220000000000000000000000000000000000000000000000000000000000000000",
        "layer list",
        "layer show id",
        "stack create --from id",
        "stack pull id",
        "stack add --from branch@commit",
        "stack push id",
        "stack list",
        "stack show id",
        "branch create --from id",
        "branch merge source --into target",
        "branch pull id",
        "branch push id",
        "branch pull-commits id",
        "branch list",
        "branch show id",
        "branch diff left right",
        "workspace create branch --at /tmp/work",
        "workspace shell w:00000000000000000000000000000000",
        "workspace exec w:00000000000000000000000000000000 -- /bin/echo 'two words'",
        "workspace output x:00000000000000000000000000000000 --follow",
        "workspace stop x:00000000000000000000000000000000",
        "workspace commit w:00000000000000000000000000000000",
        "workspace end w:00000000000000000000000000000000 --discard",
        "workspace list",
        "workspace show w:00000000000000000000000000000000",
        "workspace diff w:00000000000000000000000000000000",
        "monitor db",
        "monitor dedup",
        "monitor workspace",
        "monitor branch id",
        "monitor operation",
        "monitor process",
    ] {
        CliSession::parse_line(line).unwrap_or_else(|error| panic!("{line}: {error}"));
    }
    assert!(CliSession::parse_line("workspace begin branch").is_err());
    assert!(CliSession::parse_line("layer init --empty /tmp/root").is_err());
}

#[test]
fn quoted_exec_argv_is_not_joined_or_reparsed() {
    let command = CliSession::parse_line(
        "workspace exec w:00000000000000000000000000000000 -- program 'two words' '$HOME'",
    )
    .unwrap();
    let Command::Workspace {
        command: WorkspaceCommand::Exec { argv, .. },
    } = command
    else {
        panic!("workspace exec")
    };
    assert_eq!(argv[0], "program");
    assert_eq!(argv[1], "two words");
    assert_eq!(argv[2], "$HOME");
}
