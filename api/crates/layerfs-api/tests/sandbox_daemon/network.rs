//! Every namespaces Sandbox gets its own private network (spec "every
//! Sandbox gets its own private network", §7), live, through the call
//! table, on each machine the settings name:
//!
//! - the docker machine: LAYERFS_TEST_IMAGE (the worker container's local
//!   image) and LAYERFS_BUNDLE (a sealed Linux bundle of this build); the
//!   test starts its own home's worker and removes it;
//! - each VM machine in LAYERFS_TEST_MACHINES (comma separated), created and
//!   started beforehand in the home LAYERFS_TEST_MACHINE_HOME; left running.
//!
//! LAYERFS_TEST_NET_PG names, per machine, a PostgreSQL server the machine
//! reaches by a host name on a non-default port (`docker=host.docker.internal:55761,lima=host.lima.internal:55761`);
//! without it the PostgreSQL probe of check 3 says NOT_RUN.
//! LAYERFS_TEST_NET_PROXY names, per machine, an HTTP proxy on the host
//! for `git ls-remote` where direct HTTPS to GitHub is filtered
//! (`lima=http://192.168.5.2:15236`), and LAYERFS_TEST_NET_GIT another
//! public remote (`apple-container=https://gitlab.com/gitlab-org/gitlab-foss.git`). Commands use
//! what the runtime layer guarantees, `bash` (`/dev/tcp`, `/dev/udp`),
//! `busybox` and `getent`, and the Sandbox image's `python3` for ICMP: a
//! command runs with no capability, and `busybox ping` opens only a raw
//! socket (`CAP_NET_RAW`), so ICMP goes through an ICMP datagram socket,
//! which `ping_group_range` lets any group open. An image without
//! `python3` fails check 2. A missing setting says NOT_RUN and passes.
//!
//! Every probe that should not get through also shows that its tool ran
//! (a TCP refusal or timeout, never a missing command; an ICMP echo or a
//! UDP datagram that left), and has a positive control: the same probe
//! that does get through somewhere else. Every probe bounded in time runs
//! in a session of its own that its bound ends whole (`bounded`), so a
//! probe that gets no answer fails at its bound and never holds its exec
//! open: `busybox timeout` ended only the process it ran, and git's
//! `remote-https` helper outlived it with the exec's output.
//!
//! Checks, per machine: 1 two Sandboxes both bind `127.0.0.1:3000`; 2 a
//! sibling's address is unreachable by TCP, ICMP and UDP, while the same
//! TCP, ICMP and UDP probes reach B's own listener and host end and A
//! counts what reaches it (the controls); 3 outbound TCP to
//! the internet, `git ls-remote` when the image has git, and PostgreSQL by
//! the machine's host name; 4 names resolve; 5 the daemon's control port and
//! a worker's non-loopback port are closed; 7 remove and a killed daemon
//! leave no host end, a fresh call keeps the address and the listener, and
//! doctor reports a planted leftover link until it is deleted. Check 6 (the
//! Store, its B6 walls) is `sandbox_machines.rs` and the B6 harness.
#![cfg(feature = "machine")]
use layerfs_api::home::{self, paths};
use layerfs_protocol::sdk::SdkValue;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn t(s: &str) -> SdkValue {
    SdkValue::text(s)
}

fn env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

fn call(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> Result<SdkValue, (String, String)> {
    let args = SdkValue::Map(
        args.iter()
            .map(|(n, v)| ((*n).to_owned(), v.clone()))
            .collect(),
    );
    layerfs_api::call(Some(crate::root::root_of(home)), name, args, &mut |_| {})
        .map_err(|r| (r.code, r.message))
}

fn ok(home: &Path, name: &str, args: &[(&str, SdkValue)]) -> SdkValue {
    call(home, name, args).unwrap_or_else(|e| panic!("{name}: {e:?}"))
}

fn tempdir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lfs-net-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

/// A shell on machine `machine` as root, outside every Sandbox: the
/// provider's own exec, with the bundle's busybox (the worker image and
/// the apple-container guest have no `ip`).
struct Machine {
    platform: String,
    vm: String,
}

impl Machine {
    fn of(home: &Path, machine: &str) -> Self {
        let record = home::read_record(&paths::machine_record(home, machine))
            .unwrap()
            .unwrap_or(SdkValue::Null);
        let platform = record.get_text("platform").unwrap_or(machine).to_owned();
        let vm = record
            .get_text("vm_name")
            .map(str::to_owned)
            .unwrap_or_else(|| {
                let id = home::open(home).unwrap().id;
                format!("layerfs-{}-docker", &id[..8])
            });
        Self { platform, vm }
    }

    /// Runs `script`; its commands name `busybox` applets explicitly.
    fn shell(&self, script: &str) -> (bool, String) {
        let busybox = "/layerfs/bundle/runtime/bin/busybox";
        let script = &format!("export PATH=/layerfs/bundle/runtime/bin:$PATH; {script}");
        let mut command = match self.platform.as_str() {
            "lima" => {
                let mut c = std::process::Command::new("limactl");
                c.args(["shell", &self.vm, "sudo", busybox, "sh", "-c", script]);
                c
            }
            "apple-container" => {
                let mut c = std::process::Command::new("container");
                c.args(["exec", &self.vm, busybox, "sh", "-c", script]);
                c
            }
            _ => {
                let mut c = std::process::Command::new("docker");
                c.args(["exec", &self.vm, busybox, "sh", "-c", script]);
                c
            }
        };
        let out = command.output().unwrap();
        let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&out.stderr));
        (out.status.success(), text)
    }

    /// The machine's `lfs*` links.
    fn links(&self) -> Vec<String> {
        let (listed, out) = self.shell("busybox ip -o link show");
        // a failed listing is never read as "no link"
        assert!(listed && out.contains(": lo:"), "{out}");
        (out.lines())
            .filter_map(|line| line.split(": ").nth(1))
            .map(|name| name.split('@').next().unwrap_or(name).to_owned())
            .filter(|name| name.starts_with("lfs"))
            .collect()
    }

    /// Waits (bounded) until no link named `link` is left: the kernel tears
    /// a network namespace down after its last process ended.
    fn link_gone(&self, link: &str) -> bool {
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline {
            if !self.links().iter().any(|name| name == link) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        false
    }
}

/// The `<machine>=<value>` entry for `machine` in the comma-separated
/// setting `name`.
fn per_machine(name: &str, machine: &str) -> Option<String> {
    env(name).and_then(|all| {
        all.split(',')
            .find_map(|pair| pair.strip_prefix(&format!("{machine}=")).map(str::to_owned))
    })
}

/// One Sandbox with one mounted Workspace.
struct Box {
    name: String,
    ws: String,
    /// `lfs<id12>`, its host end.
    link: String,
    /// Its commands' address.
    guest: String,
}

/// Runs `command` in Workspace `ws` from `/`, so nothing it runs holds the
/// Workspace busy at unmount.
fn exec(home: &Path, ws: &str, command: &str) -> (i64, String) {
    let command = format!("cd /; {command}");
    let ran = ok(
        home,
        "workspace.exec",
        &[("workspace", t(ws)), ("command", t(&command))],
    );
    let mut text = ran.get_text("stdout").unwrap_or_default().to_owned();
    text.push_str(ran.get_text("stderr").unwrap_or_default());
    (ran.get_int("exit_code").unwrap_or(-1), text)
}

fn sandbox(home: &Path, project: &str, name: &str, machine: &str) -> Box {
    ok(
        home,
        "sandbox.create",
        &[
            ("project", t(project)),
            ("name", t(name)),
            ("machine", t(machine)),
        ],
    );
    // `sandbox get`'s long form answers the Sandbox's own network: its
    // host end, named for its runtime id, and its commands' address
    let got = ok(
        home,
        "sandbox.get",
        &[("sandbox", t(name)), ("detail", SdkValue::Bool(true))],
    );
    let network = got
        .get_text("network")
        .unwrap_or_else(|| panic!("no network in {got:?}"))
        .to_owned();
    let (link, guest) = network.split_once(' ').unwrap();
    let record = home::read_record(&paths::sandbox_record(home, name))
        .unwrap()
        .unwrap();
    let id = record.get_text("runtime_id").unwrap();
    assert_eq!(link, format!("lfs{}", &id[..12]), "{record:?}");
    let guest = guest.trim_end_matches("/31").to_owned();
    ok(
        home,
        "branch.fork",
        &[
            ("project", t(project)),
            ("name", t(name)),
            ("from", t("main")),
        ],
    );
    let mounted = ok(
        home,
        "workspace.mount",
        &[("sandbox", t(name)), ("branch", t(name))],
    );
    Box {
        name: name.to_owned(),
        ws: mounted.get_text("workspace").unwrap().to_owned(),
        link: link.to_owned(),
        guest,
    }
}

/// `script` as one shell word.
fn quoted(script: &str) -> String {
    format!("'{}'", script.replace('\'', r"'\''"))
}

/// `bounded`'s shell: `$1` the bound in seconds, `$2` the probe. The probe
/// runs in a new session (`busybox setsid` keeps its pid, which is then its
/// process group's id), with the exec's output; a watcher in another
/// session ends the probe's whole group at the bound (`SIGTERM`, `SIGKILL`
/// a second later). Once the probe's own process ended, whatever of its
/// group is left and the watcher are killed (its group, and its pid too: a
/// watcher that has not called `setsid` yet has no group of its own), so
/// nothing it started holds the exec's output open and the last `wait`
/// returns at once. Its status is the probe's: 143 when the bound ended it.
/// The shell's own notices of killed jobs go to `/dev/null`.
const BOUNDED: &str = r#"exec 3>&2 2>/dev/null
busybox setsid bash -c "$2" 2>&3 3>&- &
p=$!
busybox setsid bash -c "busybox sleep $1; kill -TERM -- -$p; busybox sleep 1; kill -KILL -- -$p" </dev/null >/dev/null 3>&- &
w=$!
wait $p
r=$?
kill -KILL -- -$p -$w $w
wait $w
exit $r"#;

/// Probe `script` (a `bash` script) ended whole after `seconds`, everything
/// it started included ([`BOUNDED`]).
fn bounded(seconds: u32, script: &str) -> String {
    format!(
        "bash -c {} bounded {seconds} {}",
        quoted(BOUNDED),
        quoted(script)
    )
}

/// The guest's elapsed time `busybox time -p` printed on its own line
/// (`real <seconds>.<hundredths>`), exactly.
fn guest_elapsed(text: &str) -> Option<Duration> {
    let printed = text.lines().find_map(|line| line.strip_prefix("real "))?;
    let (seconds, hundredths) = printed.trim_end().split_once('.')?;
    if hundredths.len() != 2 {
        return None;
    }
    let ms = seconds.parse::<u64>().ok()? * 1000 + hundredths.parse::<u64>().ok()? * 10;
    Some(Duration::from_millis(ms))
}

/// `bash`'s TCP connect to `at` (`host/port`), ended after 3 s: `rc=0`
/// connected, `rc=1` with bash's `connect:` error refused or unreachable,
/// `rc=143` no answer (ended by its bound's TERM).
fn connect(at: &str) -> String {
    format!(
        "{}; echo rc=$?",
        bounded(3, &format!("exec 3<>/dev/tcp/{at}"))
    )
}

/// `connect`'s outcome: `Some(true)` connected, `Some(false)` refused or no
/// answer, `None` the probe itself failed (no `bash`, no `busybox`, ...).
fn connected(probe: &(i64, String)) -> Option<bool> {
    let rc = probe.1.lines().rev().find_map(|l| l.strip_prefix("rc="));
    match rc {
        Some("0") => Some(true),
        Some("1") if probe.1.contains("connect:") => Some(false),
        Some("143") => Some(false),
        _ => None,
    }
}

/// Asserts that `probe` ran and did not connect: a refusal or no answer,
/// never a tool failure, which would not connect either.
fn closed(probe: &(i64, String), what: &str) {
    assert_eq!(connected(probe), Some(false), "{what}: {probe:?}");
}

/// Two ICMP echoes to `argv[1]` through an ICMP datagram socket, which an
/// unprivileged command may open; prints `sent` once both left, then
/// `answered` (an echo reply within 3 s), `no answer` or `error <reason>`
/// (an ICMP error).
const ICMP: &str = "import socket, struct, sys
s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM, socket.IPPROTO_ICMP)
s.settimeout(3)
for seq in (1, 2):
    s.sendto(struct.pack(\"!BBHHH\", 8, 0, 0, 0, seq) + b\"layerfs\", (sys.argv[1], 0))
print(\"sent\", flush=True)
try:
    data = s.recv(1024)
    print(\"answered\" if data[0] == 0 else \"type %d\" % data[0])
except socket.timeout:
    print(\"no answer\")
except OSError as e:
    print(\"error %s\" % e)
";

/// Sends `ICMP` from Workspace `ws` to `to` and answers its outcome line,
/// after checking that the probe ran and its echoes left: a Sandbox image
/// without `python3`, or a socket the command may not open, fails here.
fn icmp(home: &Path, ws: &str, to: &str) -> String {
    let probe = format!(
        "command -v python3 >/dev/null || {{ echo NOPYTHON; exit 3; }}; python3 -c '{ICMP}' {to}"
    );
    let (code, out) = exec(home, ws, &probe);
    assert!(
        !out.contains("NOPYTHON"),
        "the Sandbox image has no python3: the ICMP probes need it for an unprivileged ICMP datagram socket (busybox ping needs CAP_NET_RAW, which commands never have)"
    );
    assert!(
        code == 0 && out.lines().next() == Some("sent"),
        "the ICMP probe to {to} did not send: {code} {out}"
    );
    out.lines().nth(1).unwrap_or_default().to_owned()
}

/// Three UDP datagrams from Workspace `ws` to `to`'s port 4001, through
/// bash's `/dev/udp`; asserts that each left.
fn udp(home: &Path, ws: &str, to: &str) {
    let sent = exec(
        home,
        ws,
        &format!("for i in 1 2 3; do echo probe > /dev/udp/{to}/4001 || exit 1; done; echo sent"),
    );
    assert_eq!(sent, (0, "sent\n".to_owned()), "UDP to {to}");
}

/// The outcome of check `check` in a doctor's long form: the row of its
/// tree that names it.
fn outcome(report: &SdkValue, check: &str) -> Option<String> {
    match report {
        SdkValue::Map(_) if report.get_text("check") == Some(check) => {
            report.get_text("outcome").map(str::to_owned)
        }
        SdkValue::Map(fields) => fields.iter().find_map(|(_, v)| outcome(v, check)),
        SdkValue::List(items) => items.iter().find_map(|v| outcome(v, check)),
        _ => None,
    }
}

/// The sum of `names` of `proto` in the Sandbox's own `/proc/net/snmp`.
fn snmp(home: &Path, ws: &str, proto: &str, names: &[&str]) -> u64 {
    let (code, out) = exec(home, ws, "cat /proc/net/snmp");
    assert_eq!(code, 0, "{out}");
    let mut lines = out.lines().filter(|l| l.starts_with(&format!("{proto}:")));
    let header: Vec<&str> = lines.next().unwrap().split_whitespace().collect();
    let values: Vec<&str> = lines.next().unwrap().split_whitespace().collect();
    names
        .iter()
        .map(|n| {
            values[header.iter().position(|h| h == n).unwrap()]
                .parse::<u64>()
                .unwrap()
        })
        .sum()
}

/// The Sandbox's network as its kernel counts it: UDP datagrams in and to
/// no port, ICMP echo requests in.
fn counters(home: &Path, ws: &str) -> (u64, u64) {
    (
        snmp(home, ws, "Udp", &["InDatagrams", "NoPorts"]),
        snmp(home, ws, "Icmp", &["InEchos"]),
    )
}

/// Checks 1 to 5 and 7 on `machine`, whose Store `store` holds project
/// `project`.
fn private_networks(home: &Path, machine: &str, project: &str) {
    let host = Machine::of(home, machine);
    let tag = std::process::id();
    let a = sandbox(home, project, &format!("na{tag}"), machine);
    let b = sandbox(home, project, &format!("nb{tag}"), machine);
    assert_ne!(a.guest, b.guest);
    eprintln!(
        "RAN {machine}: A {} {} / B {} {}",
        a.link, a.guest, b.link, b.guest
    );
    // each has its host end on the machine
    let links = host.links();
    assert!(
        links.contains(&a.link) && links.contains(&b.link),
        "{links:?}"
    );

    // 1: both bind 127.0.0.1:3000, and A's listener outlives B's bind
    let bind = "busybox httpd -p 127.0.0.1:3000 -h /tmp && echo bound";
    assert_eq!(exec(home, &a.ws, bind), (0, "bound\n".to_owned()));
    let listening = "busybox netstat -lnt | grep -c '127.0.0.1:3000 '";
    assert_eq!(exec(home, &a.ws, listening).1.trim(), "1");
    let second = exec(home, &b.ws, bind);
    assert_eq!(second, (0, "bound\n".to_owned()), "B could not bind :3000");
    assert_eq!(exec(home, &a.ws, listening).1.trim(), "1");

    // 2: A serves 0.0.0.0:4000; B reaches it neither at A's address nor on
    // its own loopback, nor by ICMP or UDP
    let serve = "busybox httpd -p 0.0.0.0:4000 -h /tmp && echo serving";
    assert_eq!(exec(home, &a.ws, serve), (0, "serving\n".to_owned()));
    // the controls: A reaches its own listener, and B its own (check 1's
    // :3000), with the probe B's checks use
    let own = exec(home, &a.ws, &connect("127.0.0.1/4000"));
    assert_eq!(connected(&own), Some(true), "A to its own :4000: {own:?}");
    let own = exec(home, &b.ws, &connect("127.0.0.1/3000"));
    assert_eq!(connected(&own), Some(true), "B to its own :3000: {own:?}");
    for at in [format!("{}/4000", a.guest), "127.0.0.1/4000".to_owned()] {
        let tried = exec(home, &b.ws, &connect(&at));
        closed(&tried, &format!("B to A at {at}"));
    }
    // the controls: B sends ICMP and UDP out (the same probes, to its own
    // host end, which answers: an echo reply, and a port unreachable that
    // B's own kernel counts), and A's counters count what reaches A (an
    // echo from the machine, whose root shell may open busybox ping's raw
    // socket; UDP A sends itself to its own address), so the sibling
    // checks below cannot pass because B sends nothing or because A counts
    // nothing
    let gateway = exec(
        home,
        &b.ws,
        "busybox ip route | busybox awk '/default/ {print $3}'",
    )
    .1;
    let gateway = gateway.trim().to_owned();
    assert!(!gateway.is_empty(), "B has no default route");
    let answered = icmp(home, &b.ws, &gateway);
    assert_eq!(answered, "answered", "B's ICMP to its host end {gateway}");
    let unreachable_before = snmp(home, &b.ws, "Icmp", &["InDestUnreachs"]);
    udp(home, &b.ws, &gateway);
    let unreachable_after = snmp(home, &b.ws, "Icmp", &["InDestUnreachs"]);
    assert!(
        unreachable_after > unreachable_before,
        "B's UDP to its host end got no answer: {unreachable_before} -> {unreachable_after}"
    );
    let (udp_before, echo_before) = counters(home, &a.ws);
    let (pinged, said) = host.shell(&format!("busybox ping -c 1 -W 2 {}", a.guest));
    udp(home, &a.ws, &a.guest);
    let (udp_after, echo_after) = counters(home, &a.ws);
    assert!(
        pinged && echo_after > echo_before,
        "A counted no echo from the machine: {said}"
    );
    assert!(
        udp_after >= udp_before + 3,
        "A counted no UDP: {udp_before} -> {udp_after}"
    );
    // B to A: B's echoes and datagrams leave, and nothing arrives
    let (udp_before, echo_before) = (udp_after, echo_after);
    let tried = icmp(home, &b.ws, &a.guest);
    assert_ne!(tried, "answered", "A answered B's ICMP echo");
    udp(home, &b.ws, &a.guest);
    let (udp_after, echo_after) = counters(home, &a.ws);
    assert_eq!(udp_after, udp_before, "a UDP datagram from B reached A");
    assert_eq!(echo_after, echo_before, "an ICMP echo from B reached A");

    // 3: out to anything: the internet on an arbitrary port, git, and
    // PostgreSQL by the machine's name for this host
    let ssh = exec(
        home,
        &a.ws,
        &bounded(10, "exec 3<>/dev/tcp/github.com/22; head -c 4 <&3"),
    );
    assert_eq!(ssh, (0, "SSH-".to_owned()), "{ssh:?}");
    // a public remote (GitHub's by default); where direct HTTPS to it is
    // filtered, git goes through the machine's HTTP proxy, an address on
    // the host its network reaches, or another public remote is named
    let proxy = per_machine("LAYERFS_TEST_NET_PROXY", machine)
        .map_or(String::new(), |at| format!("https_proxy={at} "));
    let remote = per_machine("LAYERFS_TEST_NET_GIT", machine)
        .unwrap_or_else(|| "https://github.com/git/git".to_owned());
    let has_git = exec(home, &a.ws, "command -v git >/dev/null && echo GIT");
    // the bound itself: a remote that never answers (TEST-NET-1, direct)
    // is ended by its bound (rc 143, no sooner than the bound), and the
    // exec returns, git's helper included. A network that answers
    // 192.0.2.1 at once (an ICMP unreachable, no route, a REJECT rule)
    // never reaches the bound: that control is NOT_RUN there, not a pass
    // (review 650828a686 F2); so is an image with no git.
    // "No sooner than the bound" is timed on the guest's clock, the one the
    // bound runs on: `busybox sleep` sleeps on the guest's CLOCK_MONOTONIC,
    // and `busybox time` reads that same clock before it starts the probe
    // and after the probe ended (in milliseconds; `-p` prints `real s.cc`,
    // truncated). A host stopwatch compares two clocks: a lima guest whose
    // clock ran about 1% fast (systemd-timesyncd slewing it forward, lima's
    // guest agent setting it back every 10 s) ended a correct 5 s bound in
    // 4.963 s of host time (live run aaa5f8362f). The guest's wall clock
    // (`date`) is no better: that agent steps it. The host's stopwatch stays
    // only as the hang guard (the exec returned within 15 s).
    match has_git.1.contains("GIT") {
        false => eprintln!("NOT_RUN TEST-NET-1 bound control on {machine}: the image has no git"),
        true => {
            let bound = Duration::from_secs(5);
            let asked = Instant::now();
            let silent = exec(
                home,
                &a.ws,
                &format!(
                    "busybox time -p {}; echo rc=$?",
                    bounded(5, "git ls-remote https://192.0.2.1/none HEAD")
                ),
            );
            let took = asked.elapsed();
            let guest = guest_elapsed(&silent.1)
                .unwrap_or_else(|| panic!("no `real s.cc` from busybox time: {silent:?}"));
            eprintln!(
                "BOUNDED git ls-remote to 192.0.2.1 on {machine}: {silent:?} in {guest:?} guest, {took:?} host"
            );
            assert!(!silent.1.contains("rc=0"), "{silent:?}");
            assert!(took < Duration::from_secs(15), "{took:?}: {silent:?}");
            match (silent.1.contains("rc=143"), guest >= bound) {
                (true, true) => eprintln!("RAN TEST-NET-1 bound control on {machine}"),
                (false, false) => eprintln!(
                    "NOT_RUN TEST-NET-1 bound control on {machine}: 192.0.2.1 was answered at once ({silent:?} in {guest:?} guest), so the bound never fired"
                ),
                _ => panic!(
                    "the bound did not end the probe at its bound: {silent:?} in {guest:?} guest, {took:?} host"
                ),
            }
        }
    }
    let git = exec(
        home,
        &a.ws,
        &format!(
            "command -v git >/dev/null || {{ echo NOGIT; exit 0; }}; \
             {}; r=$?; busybox cat /tmp/ls; exit $r",
            bounded(
                30,
                &format!("{proxy}git ls-remote {remote} HEAD 2>&1 >/tmp/ls")
            )
        ),
    );
    match git.1.contains("NOGIT") {
        true => eprintln!("NOT_RUN git ls-remote on {machine}: the image has no git"),
        false => assert!(git.0 == 0 && git.1.contains("HEAD"), "{proxy}{git:?}"),
    }
    eprintln!("RAN git ls-remote {remote} on {machine} {proxy}");
    let pg = per_machine("LAYERFS_TEST_NET_PG", machine);
    match pg {
        Some(at) => {
            let (name, port) = at.rsplit_once(':').unwrap();
            // PostgreSQL's SSLRequest: a server answers one byte, N or S
            let probe = bounded(
                10,
                &format!(
                    "exec 3<>/dev/tcp/{name}/{port}; \
                     printf '\\x00\\x00\\x00\\x08\\x04\\xd2\\x16\\x2f' >&3; head -c 1 <&3"
                ),
            );
            let answered = exec(home, &a.ws, &probe);
            assert!(
                matches!(answered.1.as_str(), "N" | "S"),
                "PostgreSQL at {at}: {answered:?}"
            );
            eprintln!("RAN PostgreSQL probe on {machine} at {at}: {}", answered.1);
        }
        None => eprintln!("NOT_RUN PostgreSQL probe on {machine}: LAYERFS_TEST_NET_PG names none"),
    }

    // 4: names resolve as the machine resolves them
    let name = match host.platform.as_str() {
        "lima" => Some("host.lima.internal"),
        "docker" if cfg!(target_os = "macos") => Some("host.docker.internal"),
        _ => None,
    };
    for wanted in name.into_iter().chain(["github.com"]) {
        let got = exec(home, &a.ws, &format!("getent hosts {wanted}"));
        assert_eq!(got.0, 0, "{wanted} did not resolve: {got:?}");
    }
    let resolver = exec(home, &a.ws, "cat /etc/resolv.conf").1;
    eprintln!("RESOLVER on {machine}: {}", resolver.replace('\n', " | "));

    // 5: the daemon's control port, on the machine's loopback, and a
    // worker's non-loopback port are closed to commands
    let record = host
        .shell(&format!(
            "busybox cat /layerfs/data/sandbox/{}/record.json",
            a.name
        ))
        .1;
    let port = record
        .split("\"port\":")
        .nth(1)
        .and_then(|rest| {
            rest.split(|c: char| !c.is_ascii_digit())
                .find(|s| !s.is_empty())
        })
        .unwrap_or_else(|| panic!("no daemon port in {record}"))
        .to_owned();
    // the controls: the machine lists both ports listening, so a closed
    // one is closed to the command, not missing
    let (listed, sockets) = host.shell("busybox netstat -lnt");
    assert!(
        listed && sockets.contains(&format!("127.0.0.1:{port} ")),
        "the daemon does not listen on the machine's 127.0.0.1:{port}: {sockets}"
    );
    let daemon = exec(home, &a.ws, &connect(&format!("127.0.0.1/{port}")));
    closed(&daemon, &format!("the daemon's port {port}"));
    if host.platform != "lima" {
        // Docker Desktop's and apple-container's worker listens on 0.0.0.0:7070
        assert!(
            sockets.contains("0.0.0.0:7070 ") || sockets.contains(":::7070 "),
            "the worker does not listen on the machine's :7070: {sockets}"
        );
        let addresses = exec(home, &a.ws, "getent hosts $(busybox hostname) | busybox awk '{print $1}'; busybox ip route | busybox awk '/default/ {print $3}'").1;
        for address in addresses.split_whitespace() {
            let worker = exec(home, &a.ws, &connect(&format!("{address}/7070")));
            closed(&worker, &format!("the worker at {address}"));
        }
        assert!(
            !addresses.trim().is_empty(),
            "no address of the machine found"
        );
    }

    // 7: a fresh call keeps the address and A's listener
    let again = ok(
        home,
        "sandbox.get",
        &[("sandbox", t(&a.name)), ("detail", SdkValue::Bool(true))],
    );
    let kept = format!("{} {}/31", a.link, a.guest);
    assert_eq!(again.get_text("network"), Some(kept.as_str()), "{again:?}");
    assert_eq!(again.get_text("state"), Some("ready"), "{again:?}");
    assert_eq!(exec(home, &a.ws, listening).1.trim(), "1");
    let addressed = exec(home, &a.ws, "busybox ip -4 -o addr show eth0").1;
    assert!(
        addressed.contains(&format!("{}/31", a.guest)),
        "{addressed}"
    );
    // a planted leftover (a veth pair, which every machine's kernel has)
    // is reported by doctor until it is deleted
    let planted = "lfs000000000000";
    let (made, said) = host.shell(&format!(
        "busybox ip link add {planted} type veth peer name lfs000000000001"
    ));
    assert!(made, "{said}");
    let report = ok(home, "machine.doctor", &[("machine", t(machine))]);
    let printed = format!("{report:?}");
    let (deleted, said) = host.shell(&format!("busybox ip link delete {planted}"));
    assert!(deleted, "{said}");
    assert!(
        printed.contains("machine.network") && printed.contains(planted),
        "{printed}"
    );
    assert!(
        printed.contains(&format!("ip link delete {planted}")),
        "{printed}"
    );
    // the same check ran again (the long form's tree names each check
    // made and its outcome), and lists the link no more
    let clean = ok(
        home,
        "machine.doctor",
        &[("machine", t(machine)), ("detail", SdkValue::Bool(true))],
    );
    assert_eq!(
        outcome(&clean, "machine.network").as_deref(),
        Some("pass"),
        "{clean:?}"
    );
    let clean = format!("{clean:?}");
    assert!(
        !clean.contains(planted) && !clean.contains("lfs000000000001"),
        "{clean}"
    );
    // remove: A's host end goes
    ok(home, "workspace.unmount", &[("workspace", t(&a.ws))]);
    ok(home, "sandbox.remove", &[("sandbox", t(&a.name))]);
    assert!(host.link_gone(&a.link), "{} outlived its Sandbox", a.link);
    // a killed daemon: B's init stage ends with it, and its host end goes.
    // Its Workspace is unmounted first, so no call needs its daemon after
    let unmounted = ok(home, "workspace.unmount", &[("workspace", t(&b.ws))]);
    eprintln!("UNMOUNTED B {unmounted:?}");
    let killed = host.shell(&format!(
        // the pattern is split so this script's own command line (and its
        // launcher's) does not match it
        "exec 2>/dev/null; d=layerfs-daem; n=0; for p in /proc/[0-9]*; do \
         case \"$(busybox tr '\\0' ' ' < $p/cmdline)\" in \
         *-{}/bin/${{d}}on\" \"*) busybox kill -9 ${{p#/proc/}} && n=$((n+1));; esac; done; echo killed $n",
        b.name
    ));
    assert_eq!(killed.1.trim(), "killed 1", "{killed:?}");
    assert!(host.link_gone(&b.link), "{} outlived its daemon", b.link);
    let removed = call(
        home,
        "sandbox.remove",
        &[("sandbox", t(&b.name)), ("force", SdkValue::Bool(true))],
    );
    assert!(
        removed.is_ok(),
        "B after its daemon was killed: {removed:?}"
    );
}

/// A folder with one file, for Init.
fn source(tag: &str) -> PathBuf {
    let folder = tempdir(tag);
    std::fs::write(folder.join("readme.txt"), "hello\n").unwrap();
    folder
}

#[test]
fn sandboxes_on_the_docker_machine_have_private_networks() {
    let (Some(image), Some(_)) = (env("LAYERFS_TEST_IMAGE"), env("LAYERFS_BUNDLE")) else {
        eprintln!("NOT_RUN: LAYERFS_TEST_IMAGE / LAYERFS_BUNDLE unset");
        return;
    };
    let home = tempdir("docker");
    let folder = source("docker-src");
    let id = home::open(&home).unwrap().id;
    ok(
        &home,
        "machine.update",
        &[("machine", t("docker")), ("image", t(&image))],
    );
    ok(&home, "store.create", &[("name", t("shop"))]);
    ok(
        &home,
        "project.init",
        &[
            ("project", t("shop/app")),
            ("folder", t(&folder.display().to_string())),
        ],
    );
    let outcome = std::panic::catch_unwind(|| {
        ok(&home, "machine.start", &[("machine", t("docker"))]);
        ok(
            &home,
            "store.move",
            &[("store", t("shop")), ("machine", t("docker"))],
        );
        private_networks(&home, "docker", "shop/app");
    });
    let worker = format!("layerfs-{}-docker", &id[..8]);
    for args in [
        vec!["rm", "-f", worker.as_str()],
        vec!["volume", "rm", &format!("{worker}-data")],
    ] {
        let _ = std::process::Command::new("docker").args(&args).output();
    }
    let _ = std::fs::remove_dir_all(&home);
    let _ = std::fs::remove_dir_all(&folder);
    if let Err(panic) = outcome {
        std::panic::resume_unwind(panic);
    }
}

#[test]
fn sandboxes_on_each_running_vm_machine_have_private_networks() {
    let (Some(home), Some(machines)) = (
        env("LAYERFS_TEST_MACHINE_HOME"),
        env("LAYERFS_TEST_MACHINES"),
    ) else {
        eprintln!("NOT_RUN: LAYERFS_TEST_MACHINE_HOME / LAYERFS_TEST_MACHINES unset");
        return;
    };
    let home = PathBuf::from(home);
    for machine in machines.split(',') {
        let store = format!("net{machine}{}", std::process::id()).replace('-', "");
        let project = format!("{store}/app");
        let folder = source(&format!("vm-{machine}"));
        ok(&home, "store.create", &[("name", t(&store))]);
        ok(
            &home,
            "store.move",
            &[("store", t(&store)), ("machine", t(machine))],
        );
        ok(
            &home,
            "project.init",
            &[
                ("project", t(&project)),
                ("folder", t(&folder.display().to_string())),
            ],
        );
        private_networks(&home, machine, &project);
        ok(&home, "store.remove", &[("store", t(&store))]);
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
