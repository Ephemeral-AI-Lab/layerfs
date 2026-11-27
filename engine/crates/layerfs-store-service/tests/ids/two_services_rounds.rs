//! External #768 check of the id return paths under interleaving, not the
//! author's tests: two Store services on one Store open, bind, commit (one
//! at a time or both at once on two threads), unmount and close in a seeded
//! random order for many rounds (30 on SQLite, 10 per seed on PostgreSQL), on SQLite and on
//! PostgreSQL. Each service
//! keeps up to two Workspaces on three shared Branches, so Commits merge,
//! handles reserve blocks after each other, and returns (Commit failure,
//! slot taken, close) meet counters another handle moved.
//!
//! After every round, from the private database: no object located twice,
//! no two inode value blocks overlapping, no locator into a missing pack,
//! no value or pack past its allocator (`verify768` on PostgreSQL, the same
//! SQL on SQLite), amcheck (PostgreSQL) or `PRAGMA integrity_check`
//! (SQLite), and the Store's own full audit. At the end every Branch, read
//! by a fresh service, holds exactly the seed plus every file a Commit
//! answered `Committed` for. The seed of each run is printed; a failure
//! names its round and operation.
//!
//! The PostgreSQL half needs `LAYERFS_TEST_POSTGRES_URL`; NOT_RUN without it.
use crate::adversary;
use crate::verify768x;

use adversary::{
    apply, commit, seed_tree, verdict, write, Gate, Node, Op, Service, TestStore, Tree, Verdict,
};
use layerfs_history::BranchId;
use layerfs_store_service::store::BoundWorkspace;
use verify768x::{alloc, allocation_failures, Rng};

const SEED: &[(&str, &[u8])] = &[("/seed", b"seed\n")];
const BRANCHES: usize = 3;
/// Rounds per run. PostgreSQL runs fewer: the Store's full audit, run after
/// every round, costs about 1 s more per round as history grows (0.9 s at
/// round 0, 16 s at round 14 on this Mac), so ten rounds fit the 120 s
/// ceiling with two runs in parallel.
const SQLITE_ROUNDS: usize = 30;
const POSTGRES_ROUNDS: usize = 10;

struct Side {
    service: Option<Service>,
    workspaces: Vec<(BoundWorkspace, usize)>,
}

struct World {
    store: TestStore,
    branches: Vec<BranchId>,
    expected: Vec<Tree>,
    sides: [Side; 2],
    serial: usize,
    log: Vec<String>,
}

impl World {
    fn open(&mut self, s: usize) {
        if self.sides[s].service.is_none() {
            self.sides[s].service = Some(self.store.open());
            self.log.push(format!("open {s}"));
        }
    }

    fn bind(&mut self, s: usize, b: usize) {
        self.open(s);
        let workspace = self.sides[s]
            .service
            .as_ref()
            .unwrap()
            .bind(self.branches[b]);
        self.sides[s].workspaces.push((workspace, b));
        self.log.push(format!("bind {s} b{b}"));
    }

    /// New files on workspace `w` of side `s`; returns them.
    fn change(&mut self, s: usize, w: usize, rng: &mut Rng) -> Vec<(String, Vec<u8>)> {
        self.serial += 1;
        let dir = format!("/d{}", self.serial);
        let workspace = &self.sides[s].workspaces[w].0;
        apply(workspace, &Op::Mkdir(dir.clone())).unwrap();
        let mut files = Vec::new();
        for n in 0..1 + rng.below(20) {
            let path = format!("{dir}/f{n}");
            let bytes = format!("{path} {}\n", rng.next()).into_bytes();
            write(workspace, &path, &bytes);
            files.push((path, bytes));
        }
        files
    }

    fn record(&mut self, b: usize, files: &[(String, Vec<u8>)], dir: &str) {
        self.expected[b].insert(dir.to_owned(), Node::Dir);
        for (path, bytes) in files {
            self.expected[b].insert(path.clone(), Node::File(bytes.clone()));
        }
    }

    fn commit_one(&mut self, s: usize, w: usize, rng: &mut Rng) {
        let files = self.change(s, w, rng);
        let dir = files[0].0.rsplit_once('/').unwrap().0.to_owned();
        let (workspace, b) = &self.sides[s].workspaces[w];
        let b = *b;
        let answer = verdict(&commit(workspace));
        self.log.push(format!(
            "commit {s}/{w} b{b} {} files: {answer:?}",
            files.len()
        ));
        assert!(
            matches!(answer, Verdict::Committed(_)),
            "{:?}: {:#?}",
            self.store.kind,
            self.log
        );
        self.record(b, &files, &dir);
    }

    /// Both sides' first Workspaces commit at once on two threads.
    fn commit_both(&mut self, rng: &mut Rng) {
        let a = self.change(0, 0, rng);
        let b = self.change(1, 0, rng);
        let gate = Gate::new(2);
        let (wa, ba) = &self.sides[0].workspaces[0];
        let (wb, bb) = &self.sides[1].workspaces[0];
        let (ba, bb) = (*ba, *bb);
        let (ra, rb) = std::thread::scope(|scope| {
            let gate = &gate;
            let ta = scope.spawn(move || {
                assert!(gate.wait());
                verdict(&commit(wa))
            });
            let tb = scope.spawn(move || {
                assert!(gate.wait());
                verdict(&commit(wb))
            });
            (ta.join().unwrap(), tb.join().unwrap())
        });
        self.log.push(format!(
            "commit both: 0 b{ba} {} files {ra:?}; 1 b{bb} {} files {rb:?}",
            a.len(),
            b.len()
        ));
        for (answer, files, branch) in [(ra, a, ba), (rb, b, bb)] {
            assert!(
                matches!(answer, Verdict::Committed(_)),
                "{:?}: {:#?}",
                self.store.kind,
                self.log
            );
            let dir = files[0].0.rsplit_once('/').unwrap().0.to_owned();
            self.record(branch, &files, &dir);
        }
    }

    fn unmount(&mut self, s: usize, w: usize) {
        let (workspace, b) = self.sides[s].workspaces.remove(w);
        self.sides[s].service.as_ref().unwrap().unmount(workspace);
        self.log.push(format!("unmount {s} b{b}"));
    }

    fn close(&mut self, s: usize) {
        while !self.sides[s].workspaces.is_empty() {
            self.unmount(s, 0);
        }
        if let Some(service) = self.sides[s].service.take() {
            service.service.close().expect("an orderly close");
            service.owner.stop().expect("the overlay owner stops");
            self.log.push(format!("close {s}"));
        }
    }

    fn check(&self, round: usize) {
        let kind = self.store.kind;
        let started = std::time::Instant::now();
        let failures = allocation_failures(&self.store);
        let allocation_ms = started.elapsed().as_millis();
        assert!(
            failures.is_empty(),
            "{kind:?} round {round}: {failures:#?}\n{:#?}",
            self.log
        );
        let audit = self.store.integrity_failures();
        assert!(
            audit.is_empty(),
            "{kind:?} round {round}: audit {audit:#?}\n{:#?}",
            self.log
        );
        println!(
            "CHECK {{\"kind\":\"{kind:?}\",\"round\":{round},\"allocation_ms\":{allocation_ms},\"total_ms\":{}}}",
            started.elapsed().as_millis()
        );
    }
}

fn run(store: TestStore, seed: u64, rounds: usize) {
    let kind = store.kind;
    let mut rng = Rng(seed);
    let branches: Vec<BranchId> = (0..BRANCHES)
        .map(|b| store.fork(&format!("b{b}")))
        .collect();
    let mut world = World {
        store,
        branches,
        expected: vec![seed_tree(SEED); BRANCHES],
        sides: [
            Side {
                service: None,
                workspaces: Vec::new(),
            },
            Side {
                service: None,
                workspaces: Vec::new(),
            },
        ],
        serial: 0,
        log: Vec::new(),
    };
    println!("ROUNDS_SEED {{\"kind\":\"{kind:?}\",\"seed\":{seed}}}");
    let started = std::time::Instant::now();
    for round in 0..rounds {
        world.log.push(format!(
            "-- round {round} at {} ms",
            started.elapsed().as_millis()
        ));
        for _ in 0..4 + rng.below(5) {
            let s = rng.below(2) as usize;
            match rng.below(10) {
                0 => world.open(s),
                1 | 2 => {
                    if world.sides[s].workspaces.len() < 2 {
                        let b = rng.below(BRANCHES as u64) as usize;
                        world.bind(s, b);
                    }
                }
                3..=5 => {
                    if world.sides[s].workspaces.is_empty() {
                        let b = rng.below(BRANCHES as u64) as usize;
                        world.bind(s, b);
                    }
                    let w = rng.below(world.sides[s].workspaces.len() as u64) as usize;
                    world.commit_one(s, w, &mut rng);
                }
                6 | 7 => {
                    for side in 0..2 {
                        if world.sides[side].workspaces.is_empty() {
                            let b = rng.below(BRANCHES as u64) as usize;
                            world.bind(side, b);
                        }
                    }
                    world.commit_both(&mut rng);
                }
                8 => {
                    if !world.sides[s].workspaces.is_empty() {
                        let w = rng.below(world.sides[s].workspaces.len() as u64) as usize;
                        world.unmount(s, w);
                    }
                }
                _ => world.close(s),
            }
        }
        world.check(round);
        println!(
            "ROUND {{\"kind\":\"{kind:?}\",\"round\":{round},\"ms\":{},\"alloc\":\"{:?}\"}}",
            started.elapsed().as_millis(),
            alloc(&world.store)
        );
    }
    world.close(0);
    world.close(1);
    world.check(rounds);
    for (b, branch) in world.branches.iter().enumerate() {
        assert_eq!(
            world.store.tree(*branch),
            world.expected[b],
            "{kind:?}: Branch b{b} holds exactly the answered Commits' files\n{:#?}",
            world.log
        );
    }
    let end = alloc(&world.store);
    println!(
        "ROUNDS_END {{\"kind\":\"{kind:?}\",\"seed\":{seed},\"unused_ordinals\":{},\"unused_pack_ids\":{},\"alloc\":\"{end:?}\",\"operations\":{}}}",
        end.unused_ordinals(),
        end.unused_packs(),
        world.log.len()
    );
}

#[test]
fn two_services_interleave_on_sqlite() {
    adversary::tmp();
    run(
        TestStore::sqlite("v768x-rounds-sqlite", SEED),
        0x768_5eed_0001,
        SQLITE_ROUNDS,
    );
}

#[test]
fn two_services_interleave_on_postgresql() {
    adversary::tmp();
    if let Some(store) = TestStore::postgres("v768x-rounds-pg", SEED) {
        run(store, 0x768_5eed_0002, POSTGRES_ROUNDS);
    }
}

#[test]
fn two_services_interleave_on_postgresql_second_seed() {
    adversary::tmp();
    if let Some(store) = TestStore::postgres("v768x-rounds-pg2", SEED) {
        run(store, 0x768_5eed_0003, POSTGRES_ROUNDS);
    }
}
