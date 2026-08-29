#[cfg(target_os = "linux")]
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("layerfs-mount requires Linux FUSE");
    std::process::exit(1);
}

#[cfg(target_os = "linux")]
fn run() -> Result<(), Box<dyn std::error::Error>> {
    use layerfs_mount::run_mount;
    use layerfs_sdk::{BranchId, Direct, Stacked, Workspace};
    use std::sync::{Arc, Mutex};

    enum Topology {
        Direct(Direct),
        Stacked(Stacked),
    }

    impl Topology {
        fn workspace(
            &self,
            branch: BranchId,
            spool: &std::path::Path,
        ) -> layerfs_sdk::Result<Workspace> {
            match self {
                Self::Direct(topology) => topology.workspace(branch, spool),
                Self::Stacked(topology) => topology.workspace(branch, spool),
            }
        }
    }

    let arguments = arguments()?;
    let branch_path = required(&arguments, "branch-db")?;
    let layer_path = required(&arguments, "layer-db")?;
    if arguments.contains_key("init-stacked") {
        let (_, _, _, _, _, branch) =
            Stacked::bootstrap(branch_path, required(&arguments, "stack-db")?, layer_path)?;
        println!("{}", branch.id);
        return Ok(());
    }
    if arguments.contains_key("init-direct") {
        let (_, _, _, branch) = Direct::bootstrap(branch_path, layer_path)?;
        println!("{}", branch.id);
        return Ok(());
    }
    let mount = std::path::PathBuf::from(required(&arguments, "mount")?);
    let spool = std::path::PathBuf::from(required(&arguments, "spool")?);
    let branch_id = required(&arguments, "branch")?.parse::<BranchId>()?;
    let commit_on_exit = arguments.contains_key("commit-on-exit");
    std::fs::create_dir_all(&mount)?;
    if std::fs::read_dir(&mount)?.next().is_some() {
        return Err("mountpoint must be empty".into());
    }
    if spool.starts_with(&mount)
        || std::path::Path::new(branch_path).starts_with(&mount)
        || std::path::Path::new(layer_path).starts_with(&mount)
        || arguments
            .get("stack-db")
            .is_some_and(|path| std::path::Path::new(path).starts_with(&mount))
    {
        return Err("database and spool paths must be outside the mount".into());
    }
    let topology = if let Some(stack_path) = arguments.get("stack-db") {
        Topology::Stacked(Stacked::open(branch_path, stack_path, layer_path)?)
    } else {
        Topology::Direct(Direct::open(branch_path, layer_path)?)
    };
    let workspace = Arc::new(Mutex::new(topology.workspace(branch_id, &spool)?));
    let uid = arguments.get("uid").map_or(Ok(0), |value| value.parse())?;
    let gid = arguments.get("gid").map_or(Ok(0), |value| value.parse())?;
    run_mount(workspace.clone(), &mount, uid, gid)?;
    if commit_on_exit {
        workspace.lock().map_err(|_| "workspace lock")?.commit()?;
    } else {
        workspace.lock().map_err(|_| "workspace lock")?.discard()?;
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn arguments() -> Result<std::collections::HashMap<String, String>, Box<dyn std::error::Error>> {
    let mut output = std::collections::HashMap::new();
    let mut arguments = std::env::args().skip(1);
    while let Some(argument) = arguments.next() {
        let key = argument
            .strip_prefix("--")
            .ok_or("arguments must use --name value")?;
        if key == "commit-on-exit" || key == "init-direct" || key == "init-stacked" {
            output.insert(key.to_owned(), String::new());
        } else {
            output.insert(
                key.to_owned(),
                arguments.next().ok_or("missing argument value")?,
            );
        }
    }
    Ok(output)
}

#[cfg(target_os = "linux")]
fn required<'a>(
    arguments: &'a std::collections::HashMap<String, String>,
    key: &str,
) -> Result<&'a str, Box<dyn std::error::Error>> {
    arguments
        .get(key)
        .map(String::as_str)
        .ok_or_else(|| format!("missing --{key}").into())
}
