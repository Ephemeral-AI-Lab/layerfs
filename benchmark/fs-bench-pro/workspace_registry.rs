use super::workspace_common::{Case, Entry, Receipt};
use super::Result;
use std::path::Path;

pub(crate) const FAMILIES: [&str; 11] = [
    "payload_create_read", "tiny_file_churn", "directory_construction_traversal",
    "git_tool_workflow", "namespace_mutation", "workspace_change_locality",
    "mixed_load_bearing", "dedup_cross_file", "dedup_cdc_locality",
    "dedup_workspace_reuse", "dedup_branch_history",
];

pub(crate) fn cases() -> Vec<Case> {
    let mut rows = Vec::new();
    rows.extend(super::payload_create_read::cases());
    rows.extend(super::tiny_file_churn::cases());
    rows.extend(super::directory_construction_traversal::cases());
    rows.extend(super::git_tool_workflow::cases());
    rows.extend(super::namespace_mutation::cases());
    rows.extend(super::workspace_change_locality::cases());
    rows.extend(super::mixed_load_bearing::cases());
    rows.extend(super::dedup_cross_file::cases());
    rows.extend(super::dedup_cdc_locality::cases());
    rows.extend(super::dedup_workspace_reuse::cases());
    rows.extend(super::dedup_branch_history::cases());
    rows
}

pub(crate) fn resolve(id: &str) -> Result<Case> {
    cases().into_iter().chain(proofs()).chain(inherited()).find(|case| case.id == id).ok_or_else(|| format!("unknown Workspace case: {id}").into())
}

pub(crate) fn inherited() -> Vec<Case> { super::edit_length_changing_capped::cases() }

pub(crate) fn proofs() -> Vec<Case> {
    let mut rows=super::workspace_reliability::cases();
    rows.push(Case {id:"dedup-cdc-boundaries-proof".into(),family:"dedup_cdc_locality",tier:1,kind:"boundaries"});
    rows
}

macro_rules! dispatch_family {
    ($case:expr, $function:ident $(, $arg:expr)*) => {
        match $case.family {
            "payload_create_read" => super::payload_create_read::$function($case $(, $arg)*),
            "tiny_file_churn" => super::tiny_file_churn::$function($case $(, $arg)*),
            "directory_construction_traversal" => super::directory_construction_traversal::$function($case $(, $arg)*),
            "git_tool_workflow" => super::git_tool_workflow::$function($case $(, $arg)*),
            "namespace_mutation" => super::namespace_mutation::$function($case $(, $arg)*),
            "workspace_change_locality" => super::workspace_change_locality::$function($case $(, $arg)*),
            "mixed_load_bearing" => super::mixed_load_bearing::$function($case $(, $arg)*),
            "dedup_cross_file" => super::dedup_cross_file::$function($case $(, $arg)*),
            "dedup_cdc_locality" => super::dedup_cdc_locality::$function($case $(, $arg)*),
            "dedup_workspace_reuse" => super::dedup_workspace_reuse::$function($case $(, $arg)*),
            "dedup_branch_history" => super::dedup_branch_history::$function($case $(, $arg)*),
            other => Err(format!("unknown Workspace family: {other}").into()),
        }
    };
}

pub(crate) fn fixture(case: &Case, seed: u8) -> Result<Vec<Entry>> {
    if case.family=="edit_length_changing_capped" {return super::edit_length_changing_capped::fixture(case,seed);}
    valid_seed(seed)?;
    if case.family=="workspace_reliability" { return super::workspace_reliability::fixture(); }
    if case.kind=="boundaries" { return super::dedup_cdc_locality::boundaries(); }
    dispatch_family!(case, fixture, seed)
}

pub(crate) fn expected(case: &Case, seed: u8, step: usize) -> Result<Vec<Entry>> {
    if case.family=="edit_length_changing_capped" {return super::edit_length_changing_capped::expected(case,seed,step);}
    valid_seed(seed)?;
    if case.kind=="boundaries" { return super::dedup_cdc_locality::boundaries(); }
    dispatch_family!(case, expected, seed, step)
}

pub(crate) fn apply(case: &Case, seed: u8, step: usize, verify: bool) -> Result<Receipt> {
    valid_seed(seed)?;
    dispatch_family!(case, apply, seed, step, verify)
}

fn valid_seed(seed: u8) -> Result<()> {
    if !(1..=3).contains(&seed) { return Err("Workspace seed must be 1, 2 or 3".into()); }
    Ok(())
}

pub(crate) fn is_import(case: &Case) -> bool {
    matches!(case.family, "dedup_cross_file" | "dedup_cdc_locality")
}

pub(crate) fn steps(case: &Case) -> usize {
    if case.family == "dedup_branch_history" { case.tier } else { 1 }
}

pub(crate) fn self_check() -> Result<()> {
    let rows = cases();
    if rows.len() != 130 || rows.iter().map(|r| &r.id).collect::<std::collections::BTreeSet<_>>().len() != 130 {
        return Err("Workspace registry must have 130 unique timed IDs".into());
    }
    for (family, expected) in FAMILIES.iter().zip([8,20,12,4,4,16,4,10,20,12,20]) {
        if rows.iter().filter(|r| r.family == *family).count() != expected {
            return Err(format!("wrong membership for {family}").into());
        }
    }
    if rows.iter().any(|r| ![1,10,100,500].contains(&r.tier)) { return Err("invalid tier".into()); }
    super::workspace_common::self_check()?;
    super::dedup_workloads::self_check()?;
    super::payload_create_read::self_check()?;
    super::tiny_file_churn::self_check()?;
    super::directory_construction_traversal::self_check()?;
    super::git_tool_workflow::self_check()?;
    super::namespace_mutation::self_check()?;
    super::workspace_change_locality::self_check()?;
    super::mixed_load_bearing::self_check()?;
    super::workspace_reliability::self_check()?;
    Ok(())
}

pub(crate) fn dispatch(args: &[String]) -> Result<()> {
    if args.first().is_some_and(|command| command=="workspace-reliability-workload") {
        return super::reliability_workloads::dispatch(&args[1..]);
    }
    match args {
        [command] if command == "workspace-resource-sample" => sample_resources()?,
        [command,root,seed] if command == "workspace-git-prepare" => {
            for (key,value) in super::ordinary_workloads::prepare_git(Path::new(root),seed.parse()?)? {println!("{key}={value}");}
        }
        [command,root,id,seed] if command == "workspace-git-reference" => {
            for (key,value) in super::ordinary_workloads::prepare_git_reference(Path::new(root),&resolve(id)?,seed.parse()?)? {println!("{key}={value}");}
        }
        [command,target] if command == "workspace-git-custody-out" => {
            let mut receipt=super::ordinary_workloads::capture_git_custody(Path::new("."))?;
            let encoded=receipt.remove("repository_manifest_hex").ok_or("Git custody manifest")?;
            let bytes=(0..encoded.len()).step_by(2).map(|i|u8::from_str_radix(&encoded[i..i+2],16)).collect::<std::result::Result<Vec<_>,_>>()?;
            std::fs::write(target,bytes)?;
            for (key,value) in receipt {println!("{key}={value}");}
        }
        [command] if command == "workspace-git-custody" => {
            for (key,value) in super::ordinary_workloads::capture_git_custody(Path::new("."))? {println!("{key}={value}");}
        }
        [command,id,seed,reference] if command == "workspace-git-verify" => {
            let mut receipt=super::ordinary_workloads::verify_git(Path::new("."),&resolve(id)?,seed.parse()?,Path::new(reference))?;
            receipt.remove("repository_manifest_hex");
            for (key,value) in receipt {println!("{key}={value}");}
        }
        [command,id,state,ordinal] if command == "workspace-reliability-verify" => {
            let case=super::workspace_reliability::resolve(id)?;
            let entries=super::workspace_reliability::expected(&case,state,ordinal.parse()?)?;
            for (key,value) in super::workspace_common::verify_native(Path::new("."),&entries)? {println!("{key}={value}");}
        }
        [command] if command == "workspace-self-check" => {
            self_check()?;
            println!("registry_status=pass\ntimed_case_count=130\nsample_slot_count=390");
        }
        [command, id, seed, step, mode] if command == "workspace-apply" => {
            if !matches!(mode.as_str(), "performance" | "verify") { return Err("invalid workload mode".into()); }
            let row = apply(&resolve(id)?, seed.parse()?, step.parse()?, mode == "verify")?;
            for (key, value) in row { println!("{key}={value}"); }
        }
        [command, id, seed, step] if command == "workspace-verify-tree" => {
            let entries = expected(&resolve(id)?, seed.parse()?, step.parse()?)?;
            for (key,value) in super::workspace_common::verify_native(Path::new("."), &entries)? {
                println!("{key}={value}");
            }
        }
        _ => return Err("invalid Workspace workload arguments".into()),
    }
    Ok(())
}

fn sample_resources() -> Result<()> {
    use std::io::{Read, Seek, SeekFrom, Write};
    let fields = ["memory.current", "memory.peak", "memory.stat", "memory.events", "memory.swap.current", "pids.current", "cpu.stat"];
    let mut files = fields.iter().map(|field| std::fs::File::open(format!("/sys/fs/cgroup/{field}"))).collect::<std::io::Result<Vec<_>>>()?;
    let start = std::time::Instant::now();
    let mut buffer = String::new();
    loop {
        print!("sample_ns={}", start.elapsed().as_nanos());
        for (name,file) in fields.iter().zip(&mut files) {
            file.seek(SeekFrom::Start(0))?;
            buffer.clear(); file.read_to_string(&mut buffer)?;
            for line in buffer.lines() { print!("\t{}:{}",name,line.replace(' ',"=")); }
        }
        println!(); std::io::stdout().flush()?;
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
