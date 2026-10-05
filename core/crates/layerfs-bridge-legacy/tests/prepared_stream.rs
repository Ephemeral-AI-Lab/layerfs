//! The prepared stream: its declaration, its rows and the refusals between them.
//!
//! A prepared update states exactly what its body carries, so the body is
//! decodable from the declaration alone and a stream that lies about itself is
//! refused. These cases drive the grammar directly: the same rows through the
//! writer and the reader, then one deliberate lie per check.

use layerfs_bridge::contract::*;
use std::io::{self, Read};

struct FailingEof<'a>(&'a [u8]);
impl Read for FailingEof<'_> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if self.0.is_empty() {
            Err(io::ErrorKind::BrokenPipe.into())
        } else {
            Read::read(&mut self.0, bytes)
        }
    }
}

#[derive(Default)]
struct Recorded {
    directories: Vec<DirectoryChange>,
    identities: Vec<PreparedIdentity>,
}

impl PreparedRowSink for Recorded {
    fn directory(
        &mut self,
        parent: u64,
        changes: Vec<(Vec<u8>, Option<u64>)>,
    ) -> Result<(), Failure> {
        self.directories.push(DirectoryChange { parent, changes });
        Ok(())
    }
    fn identity(&mut self, row: PreparedIdentity) -> Result<(), Failure> {
        self.identities.push(row);
        Ok(())
    }
}

fn totals(directories: &[DirectoryChange], identities: &[PreparedIdentity]) -> PreparedTotals {
    let mut totals = PreparedTotals {
        directories: directories.len() as u64,
        identities: identities.len() as u64,
        ..PreparedTotals::default()
    };
    for row in directories {
        totals.names += row.changes.len() as u64;
        totals.name_bytes += row
            .changes
            .iter()
            .map(|(name, _)| 10 + name.len() as u64)
            .sum::<u64>();
    }
    for row in identities {
        match row {
            PreparedIdentity::Rooted { fresh, .. } => {
                if *fresh {
                    totals.fresh += 1;
                }
            }
            PreparedIdentity::DirectoryPatch { .. } => totals.patches += 1,
            PreparedIdentity::DirectoryDeclaration { .. } => {
                totals.declarations += 1;
                totals.fresh += 1;
            }
        }
    }
    totals
}

fn write(directories: &[DirectoryChange], identities: &[PreparedIdentity]) -> Vec<u8> {
    let mut out = Vec::new();
    put_stream_tag(&mut out).unwrap();
    for row in directories {
        put_directory_row(&mut out, row.parent, &row.changes).unwrap();
    }
    for row in identities {
        match row {
            PreparedIdentity::Rooted {
                serial,
                kind,
                content,
                metadata,
                fresh,
            } => {
                let role = match (*fresh, *kind) {
                    (false, 1) => ROLE_EXISTING_FILE,
                    (false, _) => ROLE_EXISTING_SYMLINK,
                    (true, 1) => ROLE_FRESH_FILE,
                    (true, _) => ROLE_FRESH_SYMLINK,
                };
                put_rooted_identity(&mut out, role, *serial, content, metadata).unwrap();
            }
            PreparedIdentity::DirectoryPatch {
                serial,
                mode,
                mtime_seconds,
                mtime_nanoseconds,
            } => put_directory_identity(
                &mut out,
                ROLE_DIRECTORY_PATCH,
                *serial,
                *mode,
                *mtime_seconds,
                *mtime_nanoseconds,
            )
            .unwrap(),
            PreparedIdentity::DirectoryDeclaration {
                serial,
                mode,
                mtime_seconds,
                mtime_nanoseconds,
            } => put_directory_identity(
                &mut out,
                ROLE_DIRECTORY_DECLARATION,
                *serial,
                *mode,
                *mtime_seconds,
                *mtime_nanoseconds,
            )
            .unwrap(),
        }
    }
    out
}

fn sample() -> (Vec<DirectoryChange>, Vec<PreparedIdentity>) {
    let directories = vec![
        DirectoryChange {
            parent: 1,
            changes: vec![(b"d".to_vec(), Some(2)), (b"gone".to_vec(), None)],
        },
        DirectoryChange {
            parent: 2,
            changes: vec![(b"a".to_vec(), Some(3)), (b"b".to_vec(), Some(4))],
        },
    ];
    // In serial order: the maintained directory, then the two files it binds.
    let identities = vec![
        PreparedIdentity::DirectoryPatch {
            serial: 1,
            mode: 0o755,
            mtime_seconds: 7,
            mtime_nanoseconds: 8,
        },
        PreparedIdentity::Rooted {
            serial: 2,
            kind: 1,
            content: [3; 32],
            metadata: [4; 32],
            fresh: true,
        },
        PreparedIdentity::Rooted {
            serial: 3,
            kind: 1,
            content: [5; 32],
            metadata: [6; 32],
            fresh: true,
        },
    ];
    (directories, identities)
}

#[test]
fn a_declared_stream_round_trips_row_for_row() {
    let (directories, identities) = sample();
    let totals = totals(&directories, &identities);
    let body = write(&directories, &identities);
    assert_eq!(totals.stream_bytes().unwrap(), body.len() as u64);
    totals.check().expect("declaration");
    let mut recorded = Recorded::default();
    read_prepared_stream(&totals, 1, &mut &body[..], &mut recorded).expect("read");
    assert_eq!(recorded.directories.len(), directories.len());
    assert_eq!(recorded.directories[1].changes[1].0, b"b".to_vec());
    assert_eq!(recorded.identities.len(), identities.len());
    assert_eq!(recorded.identities[0].serial(), 1);
}

#[test]
fn an_io_error_at_the_trailing_byte_check_is_not_eof() {
    let (directories, identities) = sample();
    let totals = totals(&directories, &identities);
    let body = write(&directories, &identities);
    let mut reader = FailingEof(&body);
    let mut recorded = Recorded::default();
    assert!(read_prepared_stream(&totals, 1, &mut reader, &mut recorded).is_err());
}

#[test]
fn a_stream_that_lies_about_its_rows_is_refused() {
    let (directories, identities) = sample();
    let totals = totals(&directories, &identities);
    let body = write(&directories, &identities);

    // One row fewer than declared.
    let short = PreparedTotals {
        identities: totals.identities - 1,
        ..totals
    };
    let mut recorded = Recorded::default();
    assert!(read_prepared_stream(&short, 1, &mut &body[..], &mut recorded).is_err());

    // One name fewer than declared.
    let fewer = PreparedTotals {
        names: totals.names - 1,
        ..totals
    };
    let mut recorded = Recorded::default();
    assert!(read_prepared_stream(&fewer, 1, &mut &body[..], &mut recorded).is_err());

    // A body with a trailing byte the declaration does not describe.
    let mut longer = body.clone();
    longer.push(0);
    let mut recorded = Recorded::default();
    assert!(read_prepared_stream(&totals, 1, &mut &longer[..], &mut recorded).is_err());

    // An unknown version.
    let mut other = body.clone();
    other[0] = 9;
    let mut recorded = Recorded::default();
    assert!(read_prepared_stream(&totals, 1, &mut &other[..], &mut recorded).is_err());
}

#[test]
fn a_stream_whose_rows_are_out_of_order_is_refused() {
    let directories = vec![
        DirectoryChange {
            parent: 5,
            changes: Vec::new(),
        },
        DirectoryChange {
            parent: 3,
            changes: Vec::new(),
        },
    ];
    let identities = vec![PreparedIdentity::Rooted {
        serial: 9,
        kind: 1,
        content: [1; 32],
        metadata: [2; 32],
        fresh: false,
    }];
    let totals = totals(&directories, &identities);
    let body = write(&directories, &identities);
    let mut recorded = Recorded::default();
    assert!(read_prepared_stream(&totals, 1, &mut &body[..], &mut recorded).is_err());
}

#[test]
fn a_declaration_past_the_charged_bound_is_refused() {
    // One row whose naming bytes alone exceed the charged stream bound.
    let totals = PreparedTotals {
        directories: 1,
        names: 1,
        name_bytes: MAX_PREPARED_STREAM_BYTES + 1,
        identities: 0,
        patches: 0,
        declarations: 0,
        fresh: 0,
    };
    assert!(totals.check().is_err());
}
