//! Opt-in public SDK diagnostic for the native control channel's silent Exec boundary.
//! This silent control is distinct from the #241 position manifest and does
//! not perform a range edit, Commit, or performance measurement.
use layerfs_api_core::WorkspaceError;
use layerfs_bridge::contract::Code;
use layerfs_sdk::{HistoryMode, ProjectApi, SandboxApi, Server, ServerConfig, WorkspaceApi};
use std::{
    fs::{self, File},
    io::Write,
    path::PathBuf,
    time::{Duration, Instant},
};

const CASE_ID: &str = "issue241-silent-exec-control-v1";
const COMMAND: &str = "sleep 6";

#[test]
fn silent_exec_exposes_native_progress_boundary() {
    let image = std::env::var("LAYERFS_EXEC_LIVENESS_IMAGE");
    let output = std::env::var_os("LAYERFS_EXEC_LIVENESS_OUTPUT");
    if image.is_err() && output.is_none() {
        return;
    }
    let image = image.expect("LAYERFS_EXEC_LIVENESS_IMAGE is required for this opt-in test");
    let output = PathBuf::from(output.expect("LAYERFS_EXEC_LIVENESS_OUTPUT is required"));
    assert!(
        image.starts_with("sha256:") && image.len() == 71,
        "immutable image ID required"
    );
    fs::create_dir(&output).expect("fresh append-only diagnostic output directory");
    let mut receipt = File::create_new(output.join("result.tsv")).unwrap();
    writeln!(receipt, "schema\tissue241-exec-liveness-diagnostic-v1").unwrap();
    writeln!(receipt, "case_id\t{CASE_ID}").unwrap();
    writeln!(receipt, "command\t{COMMAND}").unwrap();
    writeln!(receipt, "fixture_bytes\t4").unwrap();
    writeln!(receipt, "image_id\t{image}").unwrap();
    writeln!(
        receipt,
        "cache_contract\tfunctional-uncontrolled-nonadmission"
    )
    .unwrap();
    writeln!(receipt, "performance_sample\tfalse").unwrap();
    writeln!(receipt, "status\tSTARTED").unwrap();

    let source = output.join("source");
    fs::create_dir(&source).unwrap();
    fs::write(source.join("note"), b"base").unwrap();
    let server = Server::create(ServerConfig {
        store_path: output.join("store.sqlite"),
        history_path: output.join("history.sqlite"),
        binding_key: b"issue241-exec-liveness".to_vec(),
        incarnation: 1,
        cursor_key: [71; 32],
        history: HistoryMode::Create,
        service_host: "host.docker.internal".into(),
        runtime: layerfs_telemetry::runtime::Runtime::disabled(),
        telemetry_run: None,
    })
    .unwrap();
    server.listen().unwrap();
    let owner = server.owner().unwrap();
    let projects = ProjectApi::new(&server);
    let sandboxes = SandboxApi::new(&owner);
    let workspaces = WorkspaceApi::new(&owner);
    let project = projects.init(CASE_ID, &source).unwrap();
    let branch = projects.fork(&project, [71; 16], "diagnostic").unwrap();
    let sandbox = match sandboxes.create(&image, CASE_ID) {
        Ok(sandbox) => sandbox,
        Err(error) => {
            writeln!(receipt, "sandbox_create\t{error:?}").unwrap();
            if let Some(id) = error.sandbox {
                writeln!(
                    receipt,
                    "retained_sandbox_delete\t{:?}",
                    sandboxes.delete(id)
                )
                .unwrap();
            }
            panic!("diagnostic Sandbox create failed: {error:?}");
        }
    };
    writeln!(receipt, "sandbox_id\t{sandbox}").unwrap();
    let mount = match workspaces.mount(sandbox, &project, branch.id, None) {
        Ok(mount) => mount,
        Err(error) => {
            writeln!(receipt, "mount\t{error:?}").unwrap();
            let retained = match &error {
                WorkspaceError::UncertainMount { id, .. } | WorkspaceError::Retained { id, .. } => {
                    Some(id)
                }
                _ => None,
            };
            if let Some(id) = retained {
                writeln!(receipt, "retained_unmount\t{:?}", workspaces.unmount(id)).unwrap();
            }
            let delete = sandboxes.delete(sandbox);
            writeln!(receipt, "sandbox_delete\t{delete:?}").unwrap();
            panic!("mounted diagnostic setup failed: {error:?}");
        }
    };
    let started = Instant::now();
    let exec = workspaces.exec(&mount.id, COMMAND);
    let elapsed_ns = started.elapsed().as_nanos();
    writeln!(receipt, "exec_elapsed_ns\t{elapsed_ns}").unwrap();
    writeln!(receipt, "exec\t{exec:?}").unwrap();
    let unknown = matches!(&exec, Err(WorkspaceError::Failure(failure))
                           if failure.code == Code::Unknown && failure.unknown);
    writeln!(receipt, "exec_unknown\t{unknown}").unwrap();
    let idle_boundary = (Duration::from_secs(4).as_nanos()..Duration::from_secs(10).as_nanos())
        .contains(&elapsed_ns);
    writeln!(receipt, "five_second_idle_window\t{idle_boundary}").unwrap();

    // The unknown Exec is never retried. Cleanup remains a separate public SDK
    // outcome; a busy daemon can refuse the new control session for unmount.
    let unmount = workspaces.unmount(&mount.id);
    writeln!(receipt, "unmount\t{unmount:?}").unwrap();
    let delete = sandboxes.delete(sandbox);
    writeln!(receipt, "sandbox_delete\t{delete:?}").unwrap();
    let listed = sandboxes.list();
    writeln!(receipt, "sandbox_list\t{listed:?}").unwrap();
    let absent = listed
        .as_ref()
        .is_ok_and(|items| items.iter().all(|item| item.id != sandbox));
    writeln!(receipt, "sandbox_absent\t{absent}").unwrap();
    writeln!(
        receipt,
        "status\t{}",
        if unknown && idle_boundary && delete.is_ok() && absent {
            "PASS_DIAGNOSTIC"
        } else {
            "FAIL_DIAGNOSTIC"
        }
    )
    .unwrap();
    assert!(
        unknown,
        "silent Exec did not return typed Unknown: {exec:?}"
    );
    assert!(
        idle_boundary,
        "Exec did not stop near the five-second idle boundary"
    );
    assert!(
        delete.is_ok() && absent,
        "SDK Sandbox cleanup was not confirmed"
    );
}
