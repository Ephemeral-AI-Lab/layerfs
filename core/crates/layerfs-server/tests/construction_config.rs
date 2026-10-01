//! Invalid indexed construction budgets refuse before composed Store/history effects.
use layerfs_server::host::{HistoryMode, Server, ServerConfig};
use layerfs_telemetry::runtime::Runtime;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

struct Temp(PathBuf);
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn invalid_configured_budgets_do_not_create_store_history_or_scratch() {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let path = std::env::temp_dir().join(format!(
        "layerfs-construction-config-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&path).unwrap();
    let temp = Temp(path);
    std::fs::write(temp.0.join("owned-marker"), b"unchanged").unwrap();
    for budget in [0, 16 * 1024 * 1024 - 4096, 16 * 1024 * 1024 + 1, 1 << 40] {
        let config = ServerConfig {
            store_path: temp.0.join("store.sqlite"),
            history_path: temp.0.join("history.sqlite"),
            binding_key: b"config-proof".to_vec(),
            incarnation: 1,
            cursor_key: [83; 32],
            history: HistoryMode::Create,
            service_host: "127.0.0.1".into(),
            runtime: Runtime::disabled(),
            telemetry_run: None,
        };
        assert!(Server::create_with_construction_scratch(config, budget).is_err());
        assert_eq!(
            std::fs::read(temp.0.join("owned-marker")).unwrap(),
            b"unchanged"
        );
        let names = std::fs::read_dir(&temp.0)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        assert_eq!(names, vec![std::ffi::OsString::from("owned-marker")]);
    }
}
