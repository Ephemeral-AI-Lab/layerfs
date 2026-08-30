use layerfs_cli::CliSession;

fn main() {
    let input = std::env::args().skip(1).collect::<Vec<_>>().join(" ");
    if input.is_empty() {
        eprintln!("mock layerfs: provide a refined V2 command");
        std::process::exit(2);
    }
    let session = CliSession::open("mock").expect("mock session");
    let command = CliSession::parse_line(&input).unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(2);
    });
    let plan = session.plan(&command).unwrap_or_else(|error| {
        eprintln!("{error}");
        std::process::exit(1);
    });
    println!("PLAN {}: {}", plan.title, plan.summary);
    let mut handle = session.execute(command).expect("execute mock command");
    let mut failed = false;
    while let Some(event) = handle.next_event().expect("mock event") {
        failed |= matches!(
            event,
            layerfs_cli::CliEvent::Finished {
                status: layerfs_cli::FinishedStatus::Failed
                    | layerfs_cli::FinishedStatus::Interrupted,
                ..
            }
        );
        println!("{event:?}");
    }
    if failed {
        std::process::exit(1);
    }
}
