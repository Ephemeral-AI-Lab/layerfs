use super::*;
/// The Engine at `home/docker.sock`, answering one request with `status` and `body`.
pub fn engine(home: &Path, status: u16, body: &str) -> JoinHandle<String> {
    let engine = engines(home, vec![(status, body.to_owned())]);
    thread::spawn(move || engine.join().unwrap().remove(0))
}

/// The Engine at `home/docker.sock`, answering each request, one connection
/// each, with the next `(status, body)`; it returns their request heads.
pub fn engines(home: &Path, responses: Vec<(u16, String)>) -> JoinHandle<Vec<String>> {
    engines_before(home, responses, || {})
}

pub fn engines_before(
    home: &Path,
    responses: Vec<(u16, String)>,
    before: impl FnOnce() + Send + 'static,
) -> JoinHandle<Vec<String>> {
    let listener = UnixListener::bind(home.join("docker.sock")).unwrap();
    listener.set_nonblocking(true).unwrap();
    thread::spawn(move || {
        let mut before = Some(before);
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut requests = Vec::new();
        for (status, body) in responses {
            let response = format!(
                "HTTP/1.1 {status} test\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                body.len()
            );
            let stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(2))
                    }
                    Err(e) => panic!("bounded engine admission: {e}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request = String::new();
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap() == 0 || line == "\r\n" {
                    break;
                }
                request.push_str(&line);
            }
            if let Some(before) = before.take() {
                before();
            }
            (&stream).write_all(response.as_bytes()).unwrap();
            requests.push(request);
        }
        requests
    })
}

/// The container's system-call filter as the Engine carries it inside a JSON string.
const SECCOMP: &str = r#"seccomp={\"defaultAction\":\"SCMP_ACT_ALLOW\",\"syscalls\":[{\"names\":[\"unshare\",\"clone\"],\"action\":\"SCMP_ACT_ERRNO\",\"errnoRet\":1,\"args\":[{\"index\":0,\"value\":268435456,\"valueTwo\":268435456,\"op\":\"SCMP_CMP_MASKED_EQ\"}]},{\"names\":[\"clone3\"],\"action\":\"SCMP_ACT_ERRNO\",\"errnoRet\":38}]}"#;

/// The Engine's inspection of CID after its process exited (Docker
/// restarted): the recorded deployment, intact, not running, no port published.
pub fn exited() -> String {
    let image = format!("sha256:{}", "a".repeat(64));
    format!(
        r#"{{"Id":"{CID}","Image":"{image}","State":{{"Running":false,"Paused":false,"Restarting":false,"Dead":false}},"Config":{{"User":"0:0","Tty":false,"Entrypoint":["/usr/local/bin/layerfs-daemon"],"Cmd":["--config","/layerfs-local/config/daemon.setup"]}},"HostConfig":{{"Init":true,"Privileged":false,"CapAdd":["CAP_SYS_ADMIN"],"Devices":[{{"PathOnHost":"/dev/fuse","PathInContainer":"/dev/fuse","CgroupPermissions":"rwm"}}],"SecurityOpt":["no-new-privileges=true","apparmor=unconfined","{SECCOMP}"],"RestartPolicy":{{"Name":"no"}},"LogConfig":{{"Type":"json-file"}}}},"Mounts":[{{"Type":"volume","Name":"lfs-test-borrowed","Destination":"/layerfs-store","RW":true,"Driver":"local"}}],"NetworkSettings":{{"Ports":{{}}}}}}"#
    )
}
