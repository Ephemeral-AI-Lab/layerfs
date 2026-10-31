//! Opened-project event queues through the real built addon and a Disposable Store.
#![cfg(target_os = "linux")]

use std::{
    os::unix::fs::DirBuilderExt,
    path::PathBuf,
    process::{Command, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
    thread,
    time::{Duration, Instant},
};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Home(PathBuf);
impl Home {
    fn new() -> Self {
        let executable = std::env::current_exe().unwrap();
        let debug = executable.parent().unwrap().parent().unwrap();
        let root = debug.join(format!(
            "native-events-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(root.join("home"))
            .unwrap();
        std::fs::create_dir(root.join("source")).unwrap();
        std::fs::write(root.join("source/base.txt"), b"base\n").unwrap();
        std::fs::create_dir_all(root.join("store")).unwrap();
        std::fs::copy(debug.join("liblayerfs_node.so"), root.join("layerfs.node"))
            .expect("build layerfs-node before running native_events");
        Self(root)
    }

    fn node(&self, script: &str, workers: Option<&str>) {
        let mut command =
            Command::new(std::env::var_os("LAYERFS_TEST_NODE").unwrap_or_else(|| "node".into()));
        command
            .arg("-e")
            .arg(format!(
                r#"
                const assert = require('node:assert/strict');
                const native = require(process.env.LAYERFS_NATIVE);
                const home = process.env.LAYERFS_HOME;
                const storePath = home + '/store/global/store.sqlite';
                const base = {{home, project:'proj', storePath}};
                const answer = async (op, args={{}}, subscription) => native.call(op, {{...base, ...args, ...(subscription ? {{subscription}} : {{}})}});
                const value = async (op, args={{}}, subscription) => {{
                    const made = await answer(op, args, subscription);
                    assert.ok(!made.refused, `${{op}}: ${{JSON.stringify(made.refused)}}`);
                    return made.value;
                }};
                const refusal = async (op, args={{}}, subscription) => {{
                    const made = await answer(op, args, subscription);
                    assert.ok(made.refused, `${{op}} unexpectedly succeeded`);
                    return made.refused;
                }};
                (async () => {{
                    const initialized = await value('project.init', {{name:'proj', path:home + '/source', timeoutMs:30000}});
                    assert.equal(initialized.kind, 'Initialized');
                    await native.storeOpen(base);
                    {script}
                    await native.storeClose(base);
                }})().catch(error => {{ console.error(error); process.exitCode = 1; }});
                "#
            ))
            .env("LAYERFS_NATIVE", self.0.join("layerfs.node"))
            .env("LAYERFS_HOME", &self.0)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        if let Some(workers) = workers {
            command.env("UV_THREADPOOL_SIZE", workers);
        }
        let mut child = command.spawn().unwrap();
        let deadline = Instant::now() + Duration::from_secs(60);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if Instant::now() >= deadline {
                child.kill().unwrap();
                child.wait().unwrap();
                panic!("native event script exceeded 60 seconds");
            }
            thread::sleep(Duration::from_millis(2));
        };
        assert!(
            status.success(),
            "Node native event assertions failed: {status}"
        );
    }
}
impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn real_store_event_mappings_snapshots_and_queue_lifetime() {
    let home = Home::new();
    home.node(
        r#"
        const first = await value('events.subscribe');
        assert.match(first.subscription, /^[0-9a-f]{64}$/);
        assert.equal(first.cursor, 0);
        assert.deepEqual(first.snapshot.branches.map(({name, head}) => ({name, head})), [{name:'main', head:null}]);
        assert.deepEqual(first.snapshot.workspaces, []);
        assert.deepEqual(first.snapshot.sandboxes, []);
        assert.deepEqual(await value('events.poll', {subscription:first.subscription, cursor:0}), {events:[], cursor:0, hasMore:false, stale:false, closed:false});
        const second = await value('events.subscribe');
        const id = first.subscription;
        const otherId = second.subscription;
        const expected = [];

        const forked = await value('branch.fork', {name:'feature', from:'main'}, id);
        expected.push({type:'branch.forked', payload:{branch:'feature', forkPoint:{kind:'layer', id:forked.base}}});
        const file = async (op, args) => {
            const made = await value(op, args, id);
            assert.equal(made.kind, 'Committed');
            const moving = op === 'fs.mv' || op === 'fs.cp';
            const plain = ['fs.write', 'fs.edit', 'fs.mkdir', 'fs.rm'].includes(op);
            expected.push({type:'fs.committed', payload:{branch:made.branch, method:op, path:plain ? args.path : null, from:moving ? args.from : null, to:moving ? args.to : null, commit:made.commit}});
            return made;
        };
        await file('fs.write', {branch:'feature', path:'/file.txt', content:'one\n', message:'write'});
        await file('fs.apply', {branch:'feature', changes:[{op:'write', path:'/apply.txt', content:'apply\n'}], message:'apply'});
        await file('fs.edit', {branch:'feature', path:'/file.txt', oldText:'one\n', newText:'two\n', message:'edit'});
        await file('fs.mkdir', {branch:'feature', path:'/dir', recursive:true, message:'mkdir'});
        await file('fs.cp', {branch:'feature', from:'/file.txt', to:'/copy.txt', recursive:true, message:'cp'});
        await file('fs.mv', {branch:'feature', from:'/copy.txt', to:'/moved.txt', message:'mv'});
        await file('fs.rm', {branch:'feature', path:'/moved.txt', recursive:true, message:'rm'});

        const leftFork = await value('branch.fork', {name:'left', from:'feature'}, id);
        const rightFork = await value('branch.fork', {name:'right', from:'feature'}, id);
        expected.push({type:'branch.forked', payload:{branch:'left', forkPoint:{kind:'commit', id:leftFork.head}}});
        expected.push({type:'branch.forked', payload:{branch:'right', forkPoint:{kind:'commit', id:rightFork.head}}});
        const leftWrite = await file('fs.write', {branch:'left', path:'/left.txt', content:'left\n', message:'left'});
        const fast = await value('branch.merge', {branch:'left', with:'right', name:'fast', message:'fast'}, id);
        assert.equal(fast.fastForward, true);
        expected.push({type:'branch.merged', payload:{merge:fast}});
        const rightWrite = await file('fs.write', {branch:'right', path:'/right.txt', content:'right\n', message:'right'});
        const slow = await value('branch.merge', {branch:'left', with:'right', name:'slow', message:'slow'}, id);
        assert.equal(slow.fastForward, false);
        expected.push({type:'branch.merged', payload:{merge:slow}});
        const published = await value('branch.publish', {branch:'feature'}, id);
        assert.equal(published.kind, 'Added');
        expected.push({type:'branch.published', payload:{branch:published.source.branch, commit:published.source.commit, layer:published.publishedLayer.id, basedOn:published.basedOn}});
        assert.equal((await value('branch.publish', {branch:'feature'}, id)).kind, 'UpToDate');
        const slowPublished = await value('branch.publish', {branch:'slow'}, id);
        assert.equal(slowPublished.kind, 'Added');
        expected.push({type:'branch.published', payload:{branch:slowPublished.source.branch, commit:slowPublished.source.commit, layer:slowPublished.publishedLayer.id, basedOn:slowPublished.basedOn}});
        const slow2 = await value('branch.merge', {branch:'left', with:'right', name:'slow2', message:'slow2'});
        assert.equal(slow2.fastForward, false);
        assert.equal(slow2.root, slow.root);
        assert.notEqual(slow2.head, slow.head);
        assert.equal((await value('branch.publish', {branch:'slow2'}, id)).kind, 'NoChanges');

        let cursor = 0;
        const events = [];
        for (;;) {
            const page = await value('events.poll', {subscription:id, cursor, timeoutMs:0, limit:500});
            events.push(...page.events);
            cursor = page.cursor;
            if (!page.hasMore) break;
        }
        assert.equal(events.length, expected.length);
        assert.deepEqual(events.map(event => event.seq), Array.from({length:events.length}, (_, i) => i + 1));
        assert.ok(events.every(event => event.source === 'store'));
        assert.ok(events.every(event => Object.keys(event).sort().join(',') === 'payload,seq,source,type'));
        for (let i = 0; i < expected.length; i++) {
            assert.equal(events[i].type, expected[i].type);
            assert.deepEqual(events[i].payload, expected[i].payload);
        }
        assert.equal(events.filter(event => event.type === 'branch.merged').length, 2);
        assert.equal(events.some(event => event.type === 'commit.created'), false);
        assert.deepEqual(await value('events.poll', {subscription:otherId, cursor:0}), {events:[], cursor:0, hasMore:false, stale:false, closed:false});

        await value('fs.write', {branch:'feature', path:'/cross-client.txt', content:'other\n'});
        assert.deepEqual((await value('events.poll', {subscription:id, cursor})).events, []);
        await value('fs.write', {branch:'feature', path:'/other-subscription.txt', content:'other\n'}, otherId);
        const onlySecond = await value('events.poll', {subscription:otherId, cursor:0});
        assert.equal(onlySecond.events.length, 1);
        assert.equal(onlySecond.events[0].payload.path, '/other-subscription.txt');
        assert.deepEqual((await value('events.poll', {subscription:id, cursor})).events, []);

        assert.equal((await refusal('events.poll', {subscription:id, cursor:cursor + 1})).message, 'unknown cursor');
        assert.equal((await refusal('events.poll', {subscription:id, cursor:-1})).code, 'InvalidInput');
        const nullCursor = await refusal('events.poll', {subscription:id, cursor:null});
        assert.equal(nullCursor.code, 'InvalidInput');
        assert.equal(nullCursor.message, 'cursor must be an integer in 0..9007199254740991');
        assert.equal((await refusal('events.poll', {subscription:id, cursor, timeoutMs:30001})).code, 'InvalidInput');
        const nullTimeout = await refusal('events.poll', {subscription:id, cursor, timeoutMs:null});
        assert.equal(nullTimeout.code, 'InvalidInput');
        assert.equal(nullTimeout.message, 'timeoutMs must be an integer in 0..30000');
        assert.equal((await refusal('events.poll', {subscription:id, cursor, limit:0})).code, 'InvalidInput');
        const nullLimit = await refusal('events.poll', {subscription:id, cursor, limit:null});
        assert.equal(nullLimit.code, 'InvalidInput');
        assert.equal(nullLimit.message, 'limit must be an integer in 1..500');
        assert.equal((await refusal('events.poll', {subscription:'0'.repeat(64), cursor:0})).message, 'unknown subscription');
        assert.equal((await value('events.unsubscribe', {subscription:otherId})).kind, 'Unsubscribed');
        assert.equal((await refusal('events.poll', {subscription:otherId, cursor:0})).message, 'unknown subscription');
        assert.equal((await value('events.unsubscribe', {subscription:id})).kind, 'Unsubscribed');

        const fresh = await value('events.subscribe');
        assert.equal(fresh.cursor, 0);
        assert.deepEqual((await value('events.poll', {subscription:fresh.subscription, cursor:0})).events, [], 'no old queue replay');
        const names = fresh.snapshot.branches.map(branch => branch.name);
        for (const name of ['main', 'feature', 'left', 'right', 'fast', 'slow', 'slow2']) assert.ok(names.includes(name), name);
        for (const branch of fresh.snapshot.branches) {
            assert.deepEqual(Object.keys(branch).sort(), ['head', 'name']);
            assert.ok(branch.head === null || typeof branch.head === 'string');
        }
        assert.deepEqual(fresh.snapshot.workspaces, []);
        assert.deepEqual(fresh.snapshot.sandboxes, []);
        await value('fs.write', {branch:'feature', path:'/old-id.txt', content:'old\n'}, id);
        assert.deepEqual((await value('events.poll', {subscription:fresh.subscription, cursor:0})).events, []);

        const waiting = native.call('events.poll', {...base, subscription:fresh.subscription, cursor:0, timeoutMs:5000});
        assert.equal((await refusal('events.poll', {subscription:fresh.subscription, cursor:0, timeoutMs:0})).message, 'a poll is already outstanding');
        await new Promise(resolve => setTimeout(resolve, 20));
        await value('fs.write', {branch:'feature', path:'/wake.txt', content:'wake\n'}, fresh.subscription);
        const woken = await waiting;
        assert.equal(woken.value.events.length, 1);
        assert.equal(woken.value.events[0].payload.path, '/wake.txt');

        const blocked = native.call('events.poll', {...base, subscription:fresh.subscription, cursor:1, timeoutMs:5000});
        await new Promise(resolve => setTimeout(resolve, 20));
        await value('events.unsubscribe', {subscription:fresh.subscription});
        assert.deepEqual((await blocked).value, {events:[], cursor:1, hasMore:false, stale:false, closed:true});
        assert.equal((await refusal('events.poll', {subscription:fresh.subscription, cursor:1})).message, 'unknown subscription');
        "#,
        None,
    );
}

#[test]
fn positive_poll_admission_is_bounded_and_immediate_polls_still_work() {
    let home = Home::new();
    home.node(
        r#"
        const first = await value('events.subscribe');
        const second = await value('events.subscribe');
        const waiting = native.call('events.poll', {...base, subscription:first.subscription, cursor:0, timeoutMs:5000});
        await new Promise(resolve => setTimeout(resolve, 20));
        const saturated = await refusal('events.poll', {subscription:second.subscription, cursor:0, timeoutMs:1000});
        assert.equal(saturated.code, 'Busy');
        assert.match(saturated.message, /long-poll admission/);
        assert.deepEqual(await value('events.poll', {subscription:second.subscription, cursor:0, timeoutMs:0}), {events:[], cursor:0, hasMore:false, stale:false, closed:false});
        await value('events.unsubscribe', {subscription:first.subscription});
        assert.equal((await waiting).value.closed, true);
        await value('events.unsubscribe', {subscription:second.subscription});
        "#,
        Some("2"),
    );
}

#[test]
fn branch_snapshot_crosses_the_native_page_boundary() {
    let home = Home::new();
    home.node(
        r#"
        const expected = ['main'];
        for (let i = 0; i < 1001; i++) {
            const name = `p9-${String(i).padStart(4, '0')}`;
            const forked = await value('branch.fork', {name, from:'main'});
            assert.equal(forked.kind, 'Forked');
            expected.push(name);
        }
        const events = await value('events.subscribe');
        assert.equal(events.snapshot.branches.length, expected.length);
        const names = events.snapshot.branches.map(branch => branch.name);
        assert.equal(new Set(names).size, expected.length);
        assert.deepEqual(new Set(names), new Set(expected));
        for (const branch of events.snapshot.branches) {
            assert.deepEqual(Object.keys(branch).sort(), ['head', 'name']);
            assert.equal(branch.head, null);
        }
        await value('events.unsubscribe', {subscription:events.subscription});
        "#,
        None,
    );
}
