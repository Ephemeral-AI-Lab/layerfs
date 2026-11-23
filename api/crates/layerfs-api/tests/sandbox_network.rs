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
//! only what the runtime layer guarantees: `bash` (`/dev/tcp`, `/dev/udp`),
//! `busybox` and `getent`. A missing setting says NOT_RUN and passes.
//!
//! Checks, per machine: 1 two Sandboxes both bind `127.0.0.1:3000`; 2 a
//! sibling's address is unreachable by TCP, ICMP and UDP; 3 outbound TCP to
//! the internet, `git ls-remote` when the image has git, and PostgreSQL by
//! the machine's host name; 4 names resolve; 5 the daemon's control port and
//! a worker's non-loopback port are closed; 7 remove and a killed daemon
//! leave no host end, a fresh call keeps the address and the listener, and
//! doctor reports a planted leftover link until it is deleted. Check 6 (the
//! Store, its B6 walls) is `sandbox_machines.rs` and the B6 harness.
#![cfg(feature = "machine")]
use layerfs_api::home::{self, paths};
use layerfs_bridge::control::SdkValue;
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
    layerfs_api::call(Some(home), name, args, &mut |_| {}).map_err(|r| (r.code, r.message))
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

/// Runs `command` in Workspace `ws` from `/`: `busybox timeout`'s watcher
/// outlives the command it bounds, and from the Workspace it would keep
/// the Workspace busy at unmount.
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

/// The Sandbox's network as its kernel counts it: UDP datagrams in and to
/// no port, ICMP echo requests in.
fn counters(home: &Path, ws: &str) -> (u64, u64) {
    let (code, out) = exec(home, ws, "cat /proc/net/snmp");
    assert_eq!(code, 0, "{out}");
    let field = |proto: &str, names: &[&str]| -> u64 {
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
    };
    (
        field("Udp", &["InDatagrams", "NoPorts"]),
        field("Icmp", &["InEchos"]),
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
    let connect = |at: &str| {
        // refused or dropped (ended by the timeout): never `reached`
        format!("busybox timeout 3 bash -c 'exec 3<>/dev/tcp/{at}' && echo reached || echo refused")
    };
    // a control: A reaches its own listener
    let own = exec(home, &a.ws, &connect("127.0.0.1/4000"));
    assert_eq!(own.1.trim(), "reached", "{own:?}");
    for at in [format!("{}/4000", a.guest), "127.0.0.1/4000".to_owned()] {
        let tried = exec(home, &b.ws, &connect(&at));
        assert!(
            !tried.1.contains("reached"),
            "B reached A at {at}: {tried:?}"
        );
    }
    let (udp_before, echo_before) = counters(home, &a.ws);
    let ping = exec(
        home,
        &b.ws,
        &format!("busybox ping -c 2 -W 2 {} && echo answered", a.guest),
    );
    assert!(!ping.1.contains("answered"), "{ping:?}");
    let udp = format!(
        "for i in 1 2 3; do echo probe > /dev/udp/{}/4001; done; echo sent",
        a.guest
    );
    exec(home, &b.ws, &udp);
    let (udp_after, echo_after) = counters(home, &a.ws);
    assert_eq!(udp_after, udp_before, "a UDP datagram from B reached A");
    assert_eq!(echo_after, echo_before, "an ICMP echo from B reached A");

    // 3: out to anything: the internet on an arbitrary port, git, and
    // PostgreSQL by the machine's name for this host
    let ssh = exec(
        home,
        &a.ws,
        "busybox timeout 10 bash -c 'exec 3<>/dev/tcp/github.com/22; head -c 4 <&3'",
    );
    assert_eq!(ssh, (0, "SSH-".to_owned()), "{ssh:?}");
    // a public remote (GitHub's by default); where direct HTTPS to it is
    // filtered, git goes through the machine's HTTP proxy, an address on
    // the host its network reaches, or another public remote is named
    let proxy = per_machine("LAYERFS_TEST_NET_PROXY", machine)
        .map_or(String::new(), |at| format!("https_proxy={at} "));
    let remote = per_machine("LAYERFS_TEST_NET_GIT", machine)
        .unwrap_or_else(|| "https://github.com/git/git".to_owned());
    let git = exec(
        home,
        &a.ws,
        &format!(
            "command -v git >/dev/null || {{ echo NOGIT; exit 0; }}; \
             {proxy}busybox timeout 30 git ls-remote {remote} HEAD 2>&1 >/tmp/ls; \
             r=$?; busybox cat /tmp/ls; exit $r"
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
            let probe = format!(
                "busybox timeout 10 bash -c \"exec 3<>/dev/tcp/{name}/{port}; printf '\\x00\\x00\\x00\\x08\\x04\\xd2\\x16\\x2f' >&3; head -c 1 <&3\""
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
    let daemon = exec(home, &a.ws, &connect(&format!("127.0.0.1/{port}")));
    assert!(
        !daemon.1.contains("reached"),
        "the daemon's port {port}: {daemon:?}"
    );
    if host.platform != "lima" {
        // Docker Desktop's and apple-container's worker listens on 0.0.0.0:7070
        let addresses = exec(home, &a.ws, "getent hosts $(busybox hostname) | busybox awk '{print $1}'; busybox ip route | busybox awk '/default/ {print $3}'").1;
        for address in addresses.split_whitespace() {
            let worker = exec(home, &a.ws, &connect(&format!("{address}/7070")));
            assert!(
                !worker.1.contains("reached"),
                "the worker at {address}: {worker:?}"
            );
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
    let clean = format!(
        "{:?}",
        ok(home, "machine.doctor", &[("machine", t(machine))])
    );
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
