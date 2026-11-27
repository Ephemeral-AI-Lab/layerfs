//! Per-opcode callback time (#352 F-032 D0): a timed callback charges its own
//! opcode with its wall time and no other opcode.
#![cfg(target_os = "linux")]
use layerfs_fuse::request::{Accounting, Opcode, OPCODES};
use std::time::{Duration, Instant};

#[test]
fn a_timed_callback_charges_only_its_opcode() {
    let accounting = std::sync::Arc::new(Accounting::default());
    assert_eq!(accounting.observe().nanos, [0; OPCODES]);
    let started = Instant::now();
    {
        let _timed = accounting.timed(Opcode::Unlink);
        std::thread::sleep(Duration::from_millis(5));
    }
    let outer = started.elapsed().as_nanos() as u64;
    {
        let _timed = accounting.timed(Opcode::Rmdir);
    }
    let work = accounting.observe();
    let unlink = work.nanos[Opcode::Unlink as usize];
    assert!(
        (5_000_000..=outer).contains(&unlink),
        "unlink {unlink} ns, outer {outer} ns"
    );
    assert!(work.nanos[Opcode::Rmdir as usize] < 5_000_000);
    for (index, nanos) in work.nanos.iter().enumerate() {
        if index != Opcode::Unlink as usize && index != Opcode::Rmdir as usize {
            assert_eq!(*nanos, 0, "{}", Opcode::NAMES[index]);
        }
    }
    // Timing never counts: frames are counted where the unit is received.
    assert_eq!(work.frames(), 0);
}
