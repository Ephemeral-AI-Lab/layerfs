//! The closed logical dispatch has no socket or native configuration dependency.
use super::{read, write};
use layerfs_bridge::contract::*;
use layerfs_storage::Store;
use layerfs_telemetry::timer::{Active, TimingScope};
use std::{
    io::{Read, Write},
    time::Instant,
};
pub fn dispatch(
    store: &Store,
    r: &Request,
    input: &mut dyn Read,
    output: &mut dyn Write,
    deadline: Instant,
    scope: &TimingScope<'_, Active>,
) -> Result<Response, Failure> {
    // A known successful C2 finish is never changed into a claimed abort.
    if r.operation.mutation() {
        write::mutate(store, r, input, deadline, scope)
    } else {
        end_input(input)?;
        read::read(store, r, output, scope)
    }
}
pub fn end_input(input: &mut dyn Read) -> Result<(), Failure> {
    let mut byte = [0; 1];
    if input.read(&mut byte)? != 0 {
        return Err(Code::InvalidInput.into());
    }
    Ok(())
}
