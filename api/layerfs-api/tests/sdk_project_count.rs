//! A project is found by its name without reading the Store's other projects
//! (#352 range 002: the SDK resolved every call's project by paging through
//! every stack, so each call would grow with the projects in a Store).
#[path = "support/installed_store.rs"]
#[allow(dead_code)]
mod installed;
#[path = "support/sdk.rs"]
#[allow(dead_code)]
mod support;

use layerfs_bridge::control::SdkValue;
use layerfs_history::{HistoryName, LayerStackId, StackInitialization};
use std::fs;
use support::Fixture;

#[test]
fn project_reads_are_independent_of_project_count() {
    let fixture = Fixture::new("sdk-project-count", |source| {
        fs::write(source.join("a.txt"), b"a\n").unwrap();
        1
    });
    let history = fixture.service.store.history();
    let installed = history
        .layer_stack(LayerStackId::from_authority([23; 16]))
        .unwrap()
        .unwrap();
    let genesis = history.layer(installed.head_layer).unwrap().unwrap().root;
    let mut made = 0usize;
    let mut observed = Vec::new();
    for count in [1usize, 600] {
        // Names that sort before `installed`, so a scan in name order reads them.
        while made + 1 < count {
            let mut authority = [0u8; 16];
            authority[..8].copy_from_slice(&(made as u64 + 1000).to_be_bytes());
            history
                .initialize_layerstack(&StackInitialization {
                    stack: LayerStackId::from_authority(authority),
                    name: HistoryName::new(&format!("aproject-{made:05}")).unwrap(),
                    scope: installed.scope,
                    profile: installed.profile,
                    genesis_root: genesis,
                    logical: 0,
                })
                .unwrap();
            made += 1;
        }
        let stat = || {
            fixture.call(
                "fs.stat",
                vec![
                    ("branch", SdkValue::text("main")),
                    ("path", SdkValue::text("/a.txt")),
                ],
            )
        };
        // The same call first, so both counts measure the same warm state.
        stat();
        let before = fixture.work();
        let stat = stat();
        let read = fixture.since(before);
        assert_eq!(stat.get_text("type"), Some("file"));
        println!("SDK_PROJECT_COUNT projects={count} stat={read:?} profile=Disposable cache=warm-repeat diagnostic_count_gate=true");
        observed.push((count, read));
    }
    let (_, few) = observed[0];
    let (_, many) = observed[1];
    assert_eq!(
        (few.statements, few.rows),
        (many.statements, many.rows),
        "a call reads every project: {observed:?}"
    );
    fixture.cleanup();
}
