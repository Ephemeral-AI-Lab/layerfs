use super::workspace_common::{Case, Entry, Receipt};
use super::Result;
use std::path::Path;

pub(crate) const FAMILIES: [&str; 16] = [
    "payload_create_read", "tiny_file_churn", "directory_construction_traversal",
    "git_tool_workflow", "namespace_mutation", "workspace_change_locality",
    "mixed_load_bearing", "dedup_cross_file", "dedup_cdc_locality",
    "dedup_workspace_reuse", "dedup_branch_history", "file_size_transition",
    "multi_workspace_development", "branch_development", "historical_access",
    "local_snapshot",
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
    rows.extend(super::file_size_transition::cases());
    rows.extend(super::multi_workspace_development::cases());
    rows.extend(super::branch_development::cases());
    rows.extend(super::historical_access::cases());
    rows.extend(super::local_snapshot::cases());
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
            "file_size_transition" => super::file_size_transition::$function($case $(, $arg)*),
            "multi_workspace_development" => super::multi_workspace_development::$function($case $(, $arg)*),
            "branch_development" => super::branch_development::$function($case $(, $arg)*),
            "historical_access" => super::historical_access::$function($case $(, $arg)*),
            "local_snapshot" => super::local_snapshot::$function($case $(, $arg)*),
            other => Err(format!("unknown Workspace family: {other}").into()),
        }
    };
}

pub(crate) fn fixture(case: &Case, seed: u8) -> Result<Vec<Entry>> {
    if case.family=="edit_length_changing_capped" {return super::edit_length_changing_capped::fixture(case,seed);}
    valid_seed(seed)?;
    if case.family=="workspace_reliability" { return super::workspace_reliability::fixture_for(case); }
    if case.kind=="boundaries" { return super::dedup_cdc_locality::boundaries(); }
    dispatch_family!(case, fixture, seed)
}

pub(crate) fn expected(case: &Case, seed: u8, step: usize) -> Result<Vec<Entry>> {
    if case.family=="edit_length_changing_capped" {return super::edit_length_changing_capped::expected(case,seed,step);}
    // v0.1.6 boundary cases declare a final state that differs from the initial
    // fixture, so their oracle comes from the family recipe and operation
    // algebra rather than from `fixture`.
    if case.family=="file_size_transition" {
        let rows = super::file_size_transition::expected(case,seed,step)?;
        return Ok(rows);
    }
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
    if case.family == "dedup_branch_history" {
        return case.tier;
    }
    if case.family == "file_size_transition" {
        return super::file_size_transition::plan(&case.id)
            .map(|plan| plan.commits)
            .unwrap_or(1);
    }
    if case.family == "local_snapshot" {
        return 3;
    }
    if case.family == "historical_access" {
        // One declared retained state is mounted; no commit is created.
        return 0;
    }
    1
}

pub(crate) fn self_check() -> Result<()> {
    let rows = cases();
    // Declared membership: 8+20+12+4+4+16+10+10+20+14+26+7+5+6+6+1. The 26
    // history rows are the 20 inherited cases plus the six v0.1.6 profiles; the
    // six branch rows are the four M1 branch forks plus the two compact graph
    // controls; the six access rows mount one selected retained state each; and
    // the sixteen families are the twelve that were registered before #154 plus
    // the three #122 families, the six #154 access additions and the sandbox
    // route's local_snapshot.
    if rows.len() != 169 || rows.iter().map(|r| &r.id).collect::<std::collections::BTreeSet<_>>().len() != 169 {
        return Err(format!("Workspace registry must have 169 unique timed IDs, observed {} rows / {} unique", rows.len(), rows.iter().map(|r| &r.id).collect::<std::collections::BTreeSet<_>>().len()).into());
    }
    let observed: Vec<(&&str, usize)> = FAMILIES
        .iter()
        .map(|family| (family, rows.iter().filter(|r| r.family == *family).count()))
        .collect();
    let declared: [usize; 16] = [8, 20, 12, 4, 4, 16, 10, 10, 20, 14, 26, 7, 5, 6, 6, 1];
    for ((family, count), expected) in observed.iter().zip(declared) {
        if *count != expected {
            return Err(format!("wrong membership for {family}: observed {count}, declared {expected}").into());
        }
    }
    if rows.iter().any(|r| ![1,10,100,500].contains(&r.tier) && r.family != "local_snapshot") { return Err("invalid tier".into()); }
    super::local_snapshot::self_check()?;
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
    super::file_size_transition::self_check()?;
    super::multi_workspace_development::self_check()?;
    super::branch_development::self_check()?;
    super::historical_access::self_check()?;
    super::v016_stages::self_check()?;
    super::v016_common::self_check()?;
    Ok(())
}

pub(crate) fn dispatch(args: &[String]) -> Result<()> {
    if args.first().is_some_and(|command| command=="workspace-reliability-workload") {
        return super::reliability_workloads::dispatch(&args[1..]);
    }
    match args {
        [command,id,seed] if command == "workspace-verify-sample" => {
            let sample = super::ordinary_workloads::workspace_sample(&resolve(id)?, seed.parse()?)?;
            let receipt = super::workspace_common::verify_native_sample(Path::new("."), &sample)?;
            for (key,value) in receipt { println!("{key}={value}"); }
        }
        [command,id,seed,step,profile,binding,covered_path,covered_sha] if command=="workspace-verify-fast-v2" => {
            if !matches!(profile.as_str(),"fully_verified"|"canonical_input_qualified"|"qualified_content_components"|"independent_current_content") {return Err("unknown fast reference assurance".into());}
            let case=resolve(id)?; let seed=seed.parse::<u8>()?; let step=step.parse::<usize>()?;
            if proofs().iter().any(|proof|proof.id==case.id) {return Err("targeted proof requires its original verification mode".into());}
            let bytes=std::fs::read(covered_path)?;
            if super::sdk_edit_common::sha256_hex(&bytes)!=*covered_sha {return Err("fast certified path projection hash mismatch".into());}
            let text=std::str::from_utf8(&bytes)?;
            let covered=text.lines().map(str::to_owned).collect::<std::collections::BTreeSet<_>>();
            if covered.iter().map(|path|format!("{path}\n")).collect::<String>()!=text {return Err("fast certified paths must be canonical sorted unique lines".into());}
            if profile=="independent_current_content"&&!covered.is_empty() {return Err("no-reference fast profile cannot claim certified content".into());}
            let entries=expected(&case,seed,step)?;
            let delta=super::ordinary_workloads::fast_delta_for_entries(&case,seed,step,&entries)?;
            let mut receipt=super::workspace_common::verify_native_fast_with_coverage(Path::new("."),&entries,&delta,binding,&covered)?;
            receipt.insert("reference_assurance".into(),profile.clone()); receipt.insert("covered_paths_sha256".into(),covered_sha.clone());
            receipt.insert("fast_witness_profile".into(),"native-fast-witness-v2:first-middle-last-seeded-per-namespace-length-depth-class;declared-read-targets".into());
            for(key,value)in receipt {println!("{key}={value}");}
        }
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
            println!("registry_status=pass\ntimed_case_count=169\nsample_slot_count=507");
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
    use std::fmt::Write as _;
    use std::io::{Read, Seek, SeekFrom, Write};
    use std::os::unix::fs::FileTypeExt;
    // Linux blocking-pipe writes up to PIPE_BUF are atomic. Bypass stdout's
    // line buffering so container shutdown cannot interrupt between row fields.
    const PIPE_BUF: usize = 4096;
    let mut output = std::fs::OpenOptions::new().write(true).open("/proc/self/fd/1")?;
    if !output.metadata()?.file_type().is_fifo() {
        return Err("resource sampler stdout must be a blocking pipe".into());
    }
    let fields = ["memory.current", "memory.peak", "memory.stat", "memory.events", "memory.swap.current", "pids.current", "cpu.stat"];
    let mut files = fields.iter().map(|field| std::fs::File::open(format!("/sys/fs/cgroup/{field}"))).collect::<std::io::Result<Vec<_>>>()?;
    let start = std::time::Instant::now();
    let mut buffer = String::new();
    let mut row = String::with_capacity(PIPE_BUF);
    loop {
        row.clear();
        write!(&mut row, "sample_ns={}", start.elapsed().as_nanos())?;
        for (name,file) in fields.iter().zip(&mut files) {
            file.seek(SeekFrom::Start(0))?;
            buffer.clear(); file.read_to_string(&mut buffer)?;
            for line in buffer.lines() { write!(&mut row, "\t{}:{}",name,line.replace(' ',"="))?; }
        }
        row.push('\n');
        if row.len() > PIPE_BUF {
            return Err(format!("resource sampler row exceeds atomic pipe bound: {} > {PIPE_BUF}", row.len()).into());
        }
        output.write_all(row.as_bytes())?;
        output.flush()?;
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
}
