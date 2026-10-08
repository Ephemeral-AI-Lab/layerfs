//! Exact transient native observer facts, including final terminal accounting.
#![cfg(target_os = "linux")]
use super::Records;
use layerfs_fuse::{mount::AbortWrite, session::SessionFacts, MountWork};
use std::io::{self, Write};

pub(super) fn emit<W: Write>(
    rows: &mut Records<'_, W>,
    facts: Option<SessionFacts>,
    drained: Option<MountWork>,
) -> io::Result<()> {
    let Some(facts) = facts else {
        for section in [12, 24, 25, 26, 27] {
            rows.row(section, 0, None)?;
        }
        return Ok(());
    };
    rows.row(
        12,
        0,
        Some(&[
            facts.mount.owner_id(),
            facts.mount.root_serial(),
            facts.serving as u64,
            facts.detached as u64,
            facts.abort_bound as u64,
        ]),
    )?;
    match facts.loops {
        Some(w) => rows.row(
            24,
            0,
            Some(&[
                w.phase as u64,
                w.revision,
                w.configured as u64,
                w.created as u64,
                w.entered as u64,
                w.exited as u64,
                w.joined as u64,
            ]),
        )?,
        None => rows.row(24, 0, None)?,
    }
    match drained.or_else(|| facts.work.ok()) {
        Some(w) => rows.row(
            25,
            0,
            Some(&[
                w.received as u64,
                w.admitted as u64,
                w.queued as u64,
                w.running as u64,
                w.parked as u64,
                w.retained as u64,
                w.owned_input_bytes as u64,
                w.future_bytes as u64,
                w.receive_units,
                w.completed,
                w.steps,
                w.wakes,
                w.terminal as u64,
            ]),
        )?,
        None => rows.row(25, 0, None)?,
    }
    let w = facts.opcodes;
    rows.row(
        26,
        0,
        Some(&[
            w.opcodes[0],
            w.opcodes[1],
            w.opcodes[2],
            w.opcodes[3],
            w.opcodes[4],
            w.opcodes[5],
            w.opcodes[6],
            w.opcodes[7],
            w.opcodes[8],
            w.opcodes[9],
            w.opcodes[10],
            w.opcodes[11],
            w.opcodes[12],
            w.opcodes[13],
            w.opcodes[14],
            w.opcodes[15],
            w.opcodes[16],
            w.opcodes[17],
            w.opcodes[18],
            w.opcodes[19],
            w.opcodes[20],
            w.opcodes[21],
            w.opcodes[22],
            w.opcodes[23],
            w.opcodes[24],
            w.opcodes[25],
            w.opcodes[26],
            w.opcodes[27],
            w.opcodes[28],
            w.opcodes[29],
            w.opcodes[30],
            w.opcodes[31],
            w.opcodes[32],
            w.opcodes[33],
            w.opcodes[34],
            w.opcodes[35],
            w.opcodes[36],
            w.opcodes[37],
            w.opcodes[38],
            w.handoffs,
            w.inline,
            w.refused,
            w.terminal,
            w.unadmitted,
            w.forget_units,
            w.store_units,
        ]),
    )?;
    match facts.aborted {
        Some(AbortWrite::Written) => rows.row(27, 0, Some(&[1, 1])),
        Some(AbortWrite::Short(count)) => rows.row(27, 0, Some(&[2, count as u64])),
        Some(AbortWrite::Failed(error)) => rows.row(27, 0, Some(&[3, error as u64])),
        None => rows.row(27, 0, None),
    }
}
