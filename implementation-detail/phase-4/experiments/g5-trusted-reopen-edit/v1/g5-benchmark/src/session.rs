const G5_REQUEST_BYTES: usize = 4_096;

#[derive(Debug)]
struct G5Request<'a> {
    id: &'a str,
    root: &'a Path,
    iteration: usize,
    warmup: bool,
    validation: RowValidation,
}

fn g5_arg_count() -> AnyResult<usize> {
    // SAFETY: Darwin owns argc for the process lifetime. We only read it.
    let argc = unsafe { *libc::_NSGetArgc() };
    usize::try_from(argc).map_err(|_| CoreError::InvalidRecord("G5 argc").into())
}

fn g5_arg(index: usize) -> AnyResult<&'static str> {
    if index >= g5_arg_count()? {
        return Err(CoreError::InvalidRecord("missing G5 argument").into());
    }
    // SAFETY: Darwin owns the checked argv pointer and NUL-terminated bytes for
    // the process lifetime. CStr::to_str validates UTF-8 without allocating.
    let value = unsafe {
        let argv = *libc::_NSGetArgv();
        if argv.is_null() {
            return Err(CoreError::InvalidRecord("G5 argv").into());
        }
        let value = *argv.add(index);
        if value.is_null() {
            return Err(CoreError::InvalidRecord("G5 argv").into());
        }
        std::ffi::CStr::from_ptr(value)
    };
    value
        .to_str()
        .map_err(|_| CoreError::InvalidRecord("G5 UTF-8 argument").into())
}

fn g5_mode(value: &str) -> AnyResult<(IntegrityMode, &'static str)> {
    match value {
        "verified" => Ok((IntegrityMode::Verified, "Verified")),
        "trusted" => Ok((IntegrityMode::TrustedLocalDev, "TrustedLocalDev")),
        _ => Err("G5 child mode must be verified or trusted".into()),
    }
}

fn g5_forecast(forecast_ns: u128, limit_ns: u128) -> AnyResult<()> {
    if limit_ns == 0 || forecast_ns > limit_ns {
        return Err("G5 full-wrapper forecast exceeds its frozen limit".into());
    }
    Ok(())
}

fn g5_operation(value: &str) -> AnyResult<&str> {
    match value {
        "first-edit-after-reopen"
        | "same-middle"
        | "one-byte-early"
        | "one-byte-middle"
        | "one-byte-late"
        | "plus1-early"
        | "plus1-middle" => Ok(value),
        _ => Err("unsupported G5 child operation".into()),
    }
}

fn g5_request_id(value: &str) -> AnyResult<&str> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err("invalid G5 request id".into());
    }
    Ok(value)
}

fn g5_request(line: &str) -> AnyResult<G5Request<'_>> {
    let mut fields = line.split('\t');
    let id = g5_request_id(fields.next().ok_or("missing G5 request id")?)?;
    let root = Path::new(fields.next().ok_or("missing G5 request root")?);
    if !root.is_absolute() {
        return Err("G5 request root must be absolute".into());
    }
    let iteration = fields
        .next()
        .ok_or("missing G5 request iteration")?
        .parse::<usize>()?;
    let warmup = fields
        .next()
        .ok_or("missing G5 request warmup")?
        .parse::<bool>()?;
    let validation = match fields.next() {
        Some("capture-only") => RowValidation::CaptureOnly,
        Some("complete-roundtrip") => RowValidation::CompleteRoundTrip,
        _ => return Err("invalid G5 request validation".into()),
    };
    if fields.next().is_some() {
        return Err("too many G5 request fields".into());
    }
    Ok(G5Request {
        id,
        root,
        iteration,
        warmup,
        validation,
    })
}

fn g5_read_line<'a>(
    input: &mut impl std::io::Read,
    buffer: &'a mut [u8; G5_REQUEST_BYTES],
) -> AnyResult<Option<&'a str>> {
    let mut used = 0;
    loop {
        if used == buffer.len() {
            return Err("G5 request exceeds fixed input bound".into());
        }
        match input.read(&mut buffer[used..used + 1])? {
            0 if used == 0 => return Ok(None),
            0 => return Err("truncated G5 request without newline".into()),
            1 if buffer[used] == b'\n' => {
                let line = std::str::from_utf8(&buffer[..used])?;
                if line.is_empty() || line.as_bytes().contains(&0) || line.ends_with('\r') {
                    return Err("invalid G5 request line".into());
                }
                return Ok(Some(line));
            }
            1 => used += 1,
            _ => unreachable!(),
        }
    }
}

pub(super) fn g5_session_main() -> AnyResult<()> {
    if g5_arg_count()? != 8 || g5_arg(1)? != "--g5-child" {
        return Err(
            "usage: --g5-child {verified|trusted} SIZE OPERATION EXPECTED_ROWS FORECAST_NS LIMIT_NS; requests on stdin"
                .into(),
        );
    }
    let (mode, mode_name) = g5_mode(g5_arg(2)?)?;
    let size = g5_arg(3)?.parse::<u64>()?;
    require_fast_size(size)?;
    let operation = g5_operation(g5_arg(4)?)?;
    let expected_rows = g5_arg(5)?.parse::<u64>()?;
    let forecast_ns = g5_arg(6)?.parse::<u128>()?;
    let limit_ns = g5_arg(7)?.parse::<u128>()?;
    g5_forecast(forecast_ns, limit_ns)?;
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    let mut buffer = [0_u8; G5_REQUEST_BYTES];
    let mut rows = 0_u64;

    writeln!(
        output,
        "{{\"status\":\"READY\",\"schema\":\"phase4-g5-trusted-child-ready-v1\",\"integrity_mode\":\"{mode_name}\",\"mode_provenance\":\"fixed-at-child-start\",\"size_bytes\":{size},\"operation\":\"{operation}\",\"expected_rows\":{expected_rows},\"full_wrapper_forecast_ns\":{forecast_ns},\"full_wrapper_limit_ns\":{limit_ns},\"forecast_status\":\"PASS\",\"request_schema\":\"id\\troot\\titeration\\twarmup\\tvalidation\"}}"
    )?;
    output.flush()?;

    while let Some(line) = g5_read_line(&mut input, &mut buffer)? {
        let request = g5_request(line)?;
        let row = run_row(
            request.root,
            SELECTED_PROFILE,
            mode,
            size,
            operation,
            request.iteration,
            request.warmup,
            request.validation,
        )?;
        writeln!(
            output,
            "{{\"status\":\"PASS\",\"schema\":\"phase4-g5-trusted-child-row-v1\",\"request_id\":\"{}\",\"integrity_mode\":\"{mode_name}\",\"mode_provenance\":\"fixed-at-child-start\",\"terminal_q_check\":\"after-emit-fail-closed\",\"row\":{}}}",
            request.id,
            &*row,
        )?;
        output.flush()?;
        drop(row);
        if q_current() != 0 {
            return Err(CoreError::LengthMismatch {
                expected: 0,
                actual: q_current(),
            }
            .into());
        }
        rows = rows.checked_add(1).ok_or(CoreError::LengthOverflow)?;
    }

    if rows != expected_rows {
        return Err(CoreError::LengthMismatch {
            expected: expected_rows,
            actual: rows,
        }
        .into());
    }
    if q_current() != 0 {
        return Err(CoreError::LengthMismatch {
            expected: 0,
            actual: q_current(),
        }
        .into());
    }
    writeln!(
        output,
        "{{\"status\":\"PASS\",\"schema\":\"phase4-g5-trusted-child-terminal-v1\",\"integrity_mode\":\"{mode_name}\",\"mode_provenance\":\"fixed-at-child-start\",\"rows\":{rows},\"expected_rows\":{expected_rows},\"argument_owners\":0,\"schedule_owners\":0,\"timing_owners\":0,\"report_owners\":0,\"q_current\":0,\"rss_terminal\":\"Unavailable: runner records external high-water; RSS is not logical Q\"}}"
    )?;
    output.flush()?;
    Ok(())
}

#[cfg(test)]
mod g5_session_tests {
    use super::*;

    #[test]
    fn request_parser_accepts_only_frozen_shape() {
        let request = g5_request("p01\t/tmp/g5\t7\tfalse\tcapture-only").expect("request");
        assert_eq!(request.id, "p01");
        assert_eq!(request.root, Path::new("/tmp/g5"));
        assert_eq!(request.iteration, 7);
        assert!(!request.warmup);
        assert_eq!(request.validation, RowValidation::CaptureOnly);
        assert!(g5_request("bad id\t/tmp/g5\t7\tfalse\tcapture-only").is_err());
        assert!(g5_request("p01\trelative\t7\tfalse\tcapture-only").is_err());
        assert!(g5_request("p01\t/tmp/g5\t7\tfalse\tcapture-only\textra").is_err());
    }

    #[test]
    fn fixed_reader_rejects_truncation_and_overflow() {
        let mut buffer = [0_u8; G5_REQUEST_BYTES];
        let mut valid = &b"p01\t/tmp/g5\t1\tfalse\tcapture-only\n"[..];
        assert!(g5_read_line(&mut valid, &mut buffer)
            .expect("line")
            .is_some());
        let mut truncated = &b"p01"[..];
        assert!(g5_read_line(&mut truncated, &mut buffer).is_err());
        let oversized = [b'x'; G5_REQUEST_BYTES + 1];
        let mut oversized = &oversized[..];
        assert!(g5_read_line(&mut oversized, &mut buffer).is_err());
    }

    #[test]
    fn process_arguments_are_borrowed_and_bounded() {
        let argc = g5_arg_count().expect("argc");
        assert!(argc > 0);
        assert!(!g5_arg(0).expect("argv0").is_empty());
        assert!(g5_arg(argc).is_err());
    }

    #[test]
    fn zero_row_forecast_is_fail_closed() {
        assert!(g5_forecast(119_999_999_999, 120_000_000_000).is_ok());
        assert!(g5_forecast(120_000_000_001, 120_000_000_000).is_err());
        assert!(g5_forecast(0, 0).is_err());
    }
}
