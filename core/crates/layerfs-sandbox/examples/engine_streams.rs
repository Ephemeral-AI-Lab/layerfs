//! Actual ordinary Engine exec/stdio/status on an explicitly caller-selected container.
use layerfs_sandbox::{
    backend::docker::{Docker, ExecRequest},
    CommandIdentity, ContainerId,
};
use std::{
    io::{self, Write},
    thread,
    time::Duration,
};
struct Expected {
    byte: u8,
    count: u64,
}
impl Write for Expected {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.iter().any(|b| *b != self.byte) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unexpected stream byte",
            ));
        }
        self.count += bytes.len() as u64;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn main() {
    let mut args = std::env::args();
    let _ = args.next();
    let socket = args.next().expect("Engine socket");
    let container = ContainerId::parse(&args.next().expect("full owned container ID")).unwrap();
    assert!(args.next().is_none());
    let docker = Docker::new(socket, Duration::from_secs(3)).unwrap();
    let security=docker.create_exec(container,ExecRequest{identity:CommandIdentity{uid:501,gid:20},arguments:vec!["/bin/bash".into(),"-c".into(),"id -u; id -g; awk '/^(Groups|CapEff|CapAmb|NoNewPrivs):/{print}' /proc/self/status".into()],environment:vec![],directory:"/tmp".into(),stdin:false}).unwrap();
    let (_, output, handle) = security.start().unwrap().into_parts().unwrap();
    let mut identity = Vec::new();
    output.copy_to(&mut identity, &mut io::sink()).unwrap();
    assert!(identity.len() < 4096);
    let identity = String::from_utf8(identity).unwrap();
    assert!(identity.starts_with("501\n20\n"));
    assert!(identity.contains("CapEff:\t0000000000000000"));
    assert!(identity.contains("CapAmb:\t0000000000000000"));
    assert!(identity.contains("NoNewPrivs:\t1"));
    assert_eq!(handle.inspect().unwrap().known_root_exit(), Some(0));
    println!("ENGINE_IDENTITY {}", identity.replace('\n', ";"));
    let request = ExecRequest {
        identity: CommandIdentity { uid: 501, gid: 20 },
        arguments: vec![
            "/bin/bash".into(),
            "-c".into(),
            "cat; printf EEEEEEEEEEEEEEEEE >&2; exit 37".into(),
        ],
        environment: vec![],
        directory: "/tmp".into(),
        stdin: true,
    };
    let created = docker.create_exec(container, request).unwrap();
    let (cid, eid) = created.identity();
    let before = docker.inspect_exec(cid, eid).unwrap();
    assert!(!before.running);
    assert_eq!(before.exit_code, None);
    let (mut input, output, handle) = created.start().unwrap().into_parts().unwrap();
    let sending = thread::spawn(move || {
        let bytes = [b'Q'; 8192];
        for _ in 0..256 {
            input.write_all(&bytes)?;
        }
        let sent = input.sent;
        let ended = input.close().unwrap();
        assert_eq!(ended.sent, sent);
        Ok::<_, io::Error>(sent)
    });
    let mut stdout = Expected {
        byte: b'Q',
        count: 0,
    };
    let mut stderr = Expected {
        byte: b'E',
        count: 0,
    };
    let done = output.copy_to(&mut stdout, &mut stderr).unwrap();
    assert_eq!(sending.join().unwrap().unwrap(), 2 * 1024 * 1024);
    assert_eq!(stdout.count, 2 * 1024 * 1024);
    assert_eq!(stderr.count, 17);
    assert_eq!(done.stdout, stdout.count);
    assert_eq!(done.stderr, stderr.count);
    let after = handle.inspect().unwrap();
    assert_eq!(after.container, cid);
    assert_eq!(after.exec, eid);
    assert_eq!(after.known_root_exit(), Some(37));
    assert!(handle.cancel().is_err());
    println!("ENGINE_STREAMS exact_container={cid} exact_exec={eid} stdin={} stdout={} stderr={} exit_code=37 prestart_exit=null transport_eof_is_not_exit=true cancellation=UNSUPPORTED_BEFORE_EFFECT no_daemon_registration=true",2*1024*1024,stdout.count,stderr.count);
}
