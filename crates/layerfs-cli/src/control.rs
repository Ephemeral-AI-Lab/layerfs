use crate::context::ContextPaths;
use crate::{
    CliError, CliEvent, CliResult, Command, CommandPlan, Completion, OperationPhase, ProgressValue,
    ViewQuery, ViewSnapshot,
};
use layerfs_sdk::OperationId;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const FRAME_LIMIT: usize = 2 * 1024 * 1024;
const MAX_CONNECTIONS: usize = 64;
const PING_REQUEST: u8 = 0;
const PLAN_REQUEST: u8 = 1;
const EXECUTE_REQUEST: u8 = 2;
const SNAPSHOT_REQUEST: u8 = 3;
const INTERRUPT_REQUEST: u8 = 4;
const PING_RESPONSE: u8 = 10;
const PLAN_RESPONSE: u8 = 11;
const ACCEPTED_RESPONSE: u8 = 12;
const EVENT_RESPONSE: u8 = 13;
const SNAPSHOT_RESPONSE: u8 = 14;
const ACK_RESPONSE: u8 = 15;
const ERROR_RESPONSE: u8 = 16;

pub struct CliSession {
    paths: ContextPaths,
}

pub struct OperationHandle {
    paths: ContextPaths,
    operation_id: OperationId,
    events: Mutex<std::sync::mpsc::Receiver<CliResult<CliEvent>>>,
}

impl CliSession {
    pub fn open(context_location: impl AsRef<Path>) -> CliResult<Self> {
        let paths = ContextPaths::new(context_location)?;
        paths.prepare()?;
        ensure_host(&paths)?;
        Ok(Self { paths })
    }

    pub fn parse_line(input: &str) -> CliResult<Command> {
        crate::parse::line(input)
    }

    pub fn plan(&self, command: &Command) -> CliResult<CommandPlan> {
        let mut request = WireWriter::new(PLAN_REQUEST);
        put_command(&mut request, command)?;
        let bytes = exchange(&self.paths, request.finish())?;
        let mut response = response(&bytes, PLAN_RESPONSE)?;
        let plan = crate::plan::get_plan(&mut response)?;
        response.done()?;
        Ok(plan)
    }

    pub fn execute(&self, command: Command) -> CliResult<OperationHandle> {
        self.execute_operation(OperationId::new(), command)
    }

    fn execute_operation(
        &self,
        operation_id: OperationId,
        command: Command,
    ) -> CliResult<OperationHandle> {
        let mut stream = connect(&self.paths)?;
        let mut request = WireWriter::new(EXECUTE_REQUEST);
        request.string(&operation_id.to_string())?;
        put_command(&mut request, &command)?;
        write_frame(&mut stream, &request.finish())?;
        let accepted = read_frame(&mut stream)?;
        response(&accepted, ACCEPTED_RESPONSE)?.done()?;

        let (sender, receiver) = std::sync::mpsc::sync_channel(64);
        std::thread::spawn(move || loop {
            let event = read_frame(&mut stream).and_then(|bytes| {
                let mut input = response(&bytes, EVENT_RESPONSE)?;
                let event = crate::event::get_event(&mut input)?;
                input.done()?;
                Ok(event)
            });
            let finished = matches!(event, Ok(CliEvent::Finished { .. }));
            if sender.send(event).is_err() || finished {
                break;
            }
        });
        Ok(OperationHandle {
            paths: self.paths.clone(),
            operation_id,
            events: Mutex::new(receiver),
        })
    }

    pub fn complete(&self, input: &str, cursor: usize) -> CliResult<Vec<Completion>> {
        if cursor > input.len() || !input.is_char_boundary(cursor) {
            return Err(CliError::Invalid("completion cursor".to_owned()));
        }
        Ok(crate::completion::complete(input, cursor))
    }

    pub fn snapshot(&self, query: ViewQuery) -> CliResult<ViewSnapshot> {
        let mut request = WireWriter::new(SNAPSHOT_REQUEST);
        crate::query::put_query(&mut request, &query)?;
        let bytes = exchange(&self.paths, request.finish())?;
        let mut response = response(&bytes, SNAPSHOT_RESPONSE)?;
        let snapshot = crate::query::get_snapshot(&mut response)?;
        response.done()?;
        Ok(snapshot)
    }
}

impl OperationHandle {
    pub fn interrupt(&self) -> CliResult<()> {
        let mut request = WireWriter::new(INTERRUPT_REQUEST);
        request.string(&self.operation_id.to_string())?;
        let bytes = exchange(&self.paths, request.finish())?;
        response(&bytes, ACK_RESPONSE)?.done()
    }

    pub fn next_event(&self) -> CliResult<Option<CliEvent>> {
        match self
            .events
            .lock()
            .map_err(|_| CliError::Context("event receiver".to_owned()))?
            .recv()
        {
            Ok(event) => event.map(Some),
            Err(_) => Ok(None),
        }
    }
}

#[doc(hidden)]
pub fn serve(context: impl AsRef<Path>) -> CliResult<()> {
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixListener;

    let paths = ContextPaths::new(context)?;
    paths.prepare()?;
    let _lock = match ContextLock::acquire(&paths)? {
        Some(lock) => lock,
        None => return Ok(()),
    };
    cleanup_socket(&paths)?;
    let host = crate::host::Host::load(ContextPaths::new(&paths.context)?)?;
    let listener = UnixListener::bind(&paths.socket).map_err(io)?;
    std::fs::set_permissions(&paths.socket, std::fs::Permissions::from_mode(0o600)).map_err(io)?;
    std::fs::write(&paths.pid, std::process::id().to_string()).map_err(io)?;
    let active = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    for stream in listener.incoming() {
        let Ok(stream) = stream else { continue };
        if !peer_is_owner(&stream, &paths)? {
            continue;
        }
        if active.fetch_add(1, std::sync::atomic::Ordering::AcqRel) >= MAX_CONNECTIONS {
            active.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
            continue;
        }
        let host = host.clone();
        let active = active.clone();
        std::thread::spawn(move || {
            let _guard = ConnectionGuard(active);
            let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
            let _ = serve_connection(stream, &host);
        });
    }
    Ok(())
}

#[doc(hidden)]
pub fn invoke(
    context: impl AsRef<Path>,
    arguments: Vec<std::ffi::OsString>,
    json_fallback: bool,
    output: &mut impl std::io::Write,
) -> CliResult<i32> {
    let total = std::time::Instant::now();
    let operation_id = OperationId::new();
    let json_hint = json_fallback || arguments.iter().any(|argument| argument == "--json");
    let parse = std::time::Instant::now();
    let parsed = crate::parse::cli(arguments);
    let parse_elapsed_ns = elapsed_ns(parse);
    let (command, json) = match parsed {
        Ok(crate::parse::CliArgv::Command(command, json)) => (command, json),
        Ok(crate::parse::CliArgv::Display(text)) => {
            let render = std::time::Instant::now();
            output.write_all(text.as_bytes()).map_err(io)?;
            output.flush().map_err(io)?;
            let render_elapsed_ns = elapsed_ns(render);
            write_invocation(
                output,
                json_hint,
                invocation_receipt(
                    operation_id,
                    layerfs_sdk::OperationOutcome::Succeeded,
                    total,
                    parse_elapsed_ns,
                    0,
                    0,
                    render_elapsed_ns,
                ),
            )?;
            return Ok(0);
        }
        Err(error) => {
            write_invocation(
                output,
                json_hint,
                invocation_receipt(
                    operation_id,
                    layerfs_sdk::OperationOutcome::Failed,
                    total,
                    parse_elapsed_ns,
                    0,
                    0,
                    0,
                ),
            )?;
            return Err(error);
        }
    };
    let context_open = std::time::Instant::now();
    let session = CliSession::open(context)?;
    let handle = session.execute_operation(operation_id, command)?;
    let context_open_elapsed_ns = elapsed_ns(context_open);
    let mut operation_wait_elapsed_ns = 0_u64;
    let mut render_elapsed_ns = 0_u64;
    loop {
        let wait = std::time::Instant::now();
        let event = handle.next_event()?.ok_or_else(|| {
            CliError::Context("operation event stream closed before Finished".to_owned())
        })?;
        operation_wait_elapsed_ns = operation_wait_elapsed_ns.saturating_add(elapsed_ns(wait));
        let finished = match &event {
            CliEvent::Finished { result, .. } => Some(result.is_ok()),
            _ => None,
        };
        let render = std::time::Instant::now();
        crate::output::render(&event, json, output)?;
        render_elapsed_ns = render_elapsed_ns.saturating_add(elapsed_ns(render));
        if let Some(succeeded) = finished {
            let render = std::time::Instant::now();
            output.flush().map_err(io)?;
            render_elapsed_ns = render_elapsed_ns.saturating_add(elapsed_ns(render));
            write_invocation(
                output,
                json,
                invocation_receipt(
                    operation_id,
                    if succeeded {
                        layerfs_sdk::OperationOutcome::Succeeded
                    } else {
                        layerfs_sdk::OperationOutcome::Failed
                    },
                    total,
                    parse_elapsed_ns,
                    context_open_elapsed_ns,
                    operation_wait_elapsed_ns,
                    render_elapsed_ns,
                ),
            )?;
            return Ok(i32::from(!succeeded));
        }
    }
}

#[doc(hidden)]
pub fn runtime_location(context: impl AsRef<Path>) -> CliResult<PathBuf> {
    Ok(ContextPaths::new(context)?.runtime)
}

fn serve_connection(
    mut stream: std::os::unix::net::UnixStream,
    host: &crate::host::Host,
) -> CliResult<()> {
    let request = read_frame(&mut stream)?;
    let mut input = WireReader::new(&request);
    match input.byte()? {
        PING_REQUEST => {
            input.done()?;
            write_frame(&mut stream, &[PING_RESPONSE])
        }
        PLAN_REQUEST => {
            let command = get_command(&mut input)?;
            input.done()?;
            match host.plan(&command) {
                Ok(plan) => {
                    let mut output = WireWriter::new(PLAN_RESPONSE);
                    crate::plan::put_plan(&mut output, &plan)?;
                    write_frame(&mut stream, &output.finish())
                }
                Err(error) => send_error(&mut stream, &error),
            }
        }
        SNAPSHOT_REQUEST => {
            let query = crate::query::get_query(&mut input)?;
            input.done()?;
            match host.snapshot(query) {
                Ok(snapshot) => {
                    let mut output = WireWriter::new(SNAPSHOT_RESPONSE);
                    crate::query::put_snapshot(&mut output, &snapshot)?;
                    write_frame(&mut stream, &output.finish())
                }
                Err(error) => send_error(&mut stream, &error),
            }
        }
        INTERRUPT_REQUEST => {
            let operation_id = parse(&input.string()?, "operation ID")?;
            input.done()?;
            match host.interrupt_operation(operation_id) {
                Ok(()) => write_frame(&mut stream, &[ACK_RESPONSE]),
                Err(error) => send_error(&mut stream, &error),
            }
        }
        EXECUTE_REQUEST => {
            let operation_id = parse(&input.string()?, "operation ID")?;
            let command = get_command(&mut input)?;
            input.done()?;
            let cancellation = match host.register_operation(operation_id) {
                Ok(cancellation) => cancellation,
                Err(error) => return send_error(&mut stream, &error),
            };
            let served = (|| {
                write_frame(&mut stream, &[ACCEPTED_RESPONSE])?;
                let send = |stream: &mut std::os::unix::net::UnixStream, event: &CliEvent| {
                    let mut output = WireWriter::new(EVENT_RESPONSE);
                    crate::event::put_event(&mut output, event)?;
                    write_frame(stream, &output.finish())
                };
                send(
                    &mut stream,
                    &CliEvent::Started {
                        operation_id,
                        command: crate::host::summary(&command),
                    },
                )?;
                send(
                    &mut stream,
                    &CliEvent::Progress {
                        operation_id,
                        phase: OperationPhase::Running,
                        progress: ProgressValue {
                            completed: 0,
                            total: None,
                        },
                        elapsed_ns: 0,
                    },
                )?;
                let mut emit = |event| send(&mut stream, &event).is_ok();
                let (_, result, receipt) =
                    host.execute(operation_id, command, cancellation.as_ref(), &mut emit);
                if let Ok(crate::CommandResult::View { scope, snapshot }) = &result {
                    send(
                        &mut stream,
                        &CliEvent::Snapshot {
                            scope: *scope,
                            snapshot: snapshot.clone(),
                        },
                    )?;
                }
                send(
                    &mut stream,
                    &CliEvent::Finished {
                        operation_id,
                        result,
                        receipt,
                    },
                )
            })();
            host.finish_operation(operation_id);
            served
        }
        _ => send_error(
            &mut stream,
            &CliError::Context("control request".to_owned()),
        ),
    }
}

fn put_command(output: &mut WireWriter, command: &Command) -> CliResult<()> {
    output.os_strings(&command.arguments())
}

fn get_command(input: &mut WireReader<'_>) -> CliResult<Command> {
    let (command, json) = crate::parse::argv(input.os_strings()?)?;
    if json {
        Err(CliError::Context("command rendering flag".to_owned()))
    } else {
        Ok(command)
    }
}

fn connect(paths: &ContextPaths) -> CliResult<std::os::unix::net::UnixStream> {
    std::os::unix::net::UnixStream::connect(&paths.socket).map_err(io)
}

fn exchange(paths: &ContextPaths, request: Vec<u8>) -> CliResult<Vec<u8>> {
    let mut stream = connect(paths)?;
    write_frame(&mut stream, &request)?;
    read_frame(&mut stream)
}

fn response(bytes: &[u8], expected: u8) -> CliResult<WireReader<'_>> {
    let mut input = WireReader::new(bytes);
    let kind = input.byte()?;
    if kind == ERROR_RESPONSE {
        let error = crate::event::get_error(&mut input)?;
        input.done()?;
        return Err(error);
    }
    if kind != expected {
        return Err(CliError::Context("control response".to_owned()));
    }
    Ok(input)
}

fn send_error(stream: &mut impl std::io::Write, error: &CliError) -> CliResult<()> {
    let mut output = WireWriter::new(ERROR_RESPONSE);
    crate::event::put_error(&mut output, error)?;
    write_frame(stream, &output.finish())
}

fn write_frame(output: &mut impl std::io::Write, bytes: &[u8]) -> CliResult<()> {
    if bytes.is_empty() || bytes.len() > FRAME_LIMIT {
        return Err(CliError::Context("control frame length".to_owned()));
    }
    output
        .write_all(&(bytes.len() as u32).to_be_bytes())
        .and_then(|()| output.write_all(bytes))
        .and_then(|()| output.flush())
        .map_err(io)
}

fn read_frame(input: &mut impl std::io::Read) -> CliResult<Vec<u8>> {
    let mut length = [0; 4];
    input.read_exact(&mut length).map_err(io)?;
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 || length > FRAME_LIMIT {
        return Err(CliError::Context("control frame length".to_owned()));
    }
    let mut bytes = vec![0; length];
    input.read_exact(&mut bytes).map_err(io)?;
    Ok(bytes)
}

pub(crate) struct WireWriter {
    bytes: Vec<u8>,
}

impl WireWriter {
    pub(crate) fn new(kind: u8) -> Self {
        Self { bytes: vec![kind] }
    }
    pub(crate) fn finish(self) -> Vec<u8> {
        self.bytes
    }
    pub(crate) fn byte(&mut self, value: u8) {
        self.bytes.push(value);
    }
    pub(crate) fn bool(&mut self, value: bool) {
        self.byte(u8::from(value));
    }
    pub(crate) fn u16(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }
    pub(crate) fn u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }
    pub(crate) fn i32(&mut self, value: i32) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }
    pub(crate) fn u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_be_bytes());
    }
    pub(crate) fn f64(&mut self, value: f64) {
        self.u64(value.to_bits());
    }
    pub(crate) fn bytes(&mut self, value: &[u8]) -> CliResult<()> {
        let len = u32::try_from(value.len())
            .map_err(|_| CliError::Context("control value length".to_owned()))?;
        self.u32(len);
        self.bytes.extend_from_slice(value);
        Ok(())
    }
    pub(crate) fn string(&mut self, value: &str) -> CliResult<()> {
        self.bytes(value.as_bytes())
    }
    pub(crate) fn path(&mut self, value: &Path) -> CliResult<()> {
        use std::os::unix::ffi::OsStrExt;
        self.bytes(value.as_os_str().as_bytes())
    }
    pub(crate) fn os_strings(&mut self, values: &[std::ffi::OsString]) -> CliResult<()> {
        self.u32(
            values
                .len()
                .try_into()
                .map_err(|_| CliError::Context("control collection length".to_owned()))?,
        );
        use std::os::unix::ffi::OsStrExt;
        values
            .iter()
            .try_for_each(|value| self.bytes(value.as_os_str().as_bytes()))
    }
}

pub(crate) struct WireReader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> WireReader<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }
    fn raw(&mut self, len: usize) -> CliResult<&'a [u8]> {
        let end = self
            .offset
            .checked_add(len)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| CliError::Context("control value".to_owned()))?;
        let value = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(value)
    }
    pub(crate) fn byte(&mut self) -> CliResult<u8> {
        Ok(self.raw(1)?[0])
    }
    pub(crate) fn bool(&mut self) -> CliResult<bool> {
        match self.byte()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(CliError::Context("control boolean".to_owned())),
        }
    }
    pub(crate) fn u16(&mut self) -> CliResult<u16> {
        Ok(u16::from_be_bytes(self.raw(2)?.try_into().expect("length")))
    }
    pub(crate) fn u32(&mut self) -> CliResult<u32> {
        Ok(u32::from_be_bytes(self.raw(4)?.try_into().expect("length")))
    }
    pub(crate) fn i32(&mut self) -> CliResult<i32> {
        Ok(i32::from_be_bytes(self.raw(4)?.try_into().expect("length")))
    }
    pub(crate) fn u64(&mut self) -> CliResult<u64> {
        Ok(u64::from_be_bytes(self.raw(8)?.try_into().expect("length")))
    }
    pub(crate) fn f64(&mut self) -> CliResult<f64> {
        Ok(f64::from_bits(self.u64()?))
    }
    pub(crate) fn bytes(&mut self) -> CliResult<&'a [u8]> {
        let len = self.u32()? as usize;
        if len > FRAME_LIMIT {
            return Err(CliError::Context("control value length".to_owned()));
        }
        self.raw(len)
    }
    pub(crate) fn string(&mut self) -> CliResult<String> {
        String::from_utf8(self.bytes()?.to_vec())
            .map_err(|_| CliError::Context("control string".to_owned()))
    }
    pub(crate) fn path(&mut self) -> CliResult<PathBuf> {
        use std::os::unix::ffi::OsStringExt;
        Ok(std::ffi::OsString::from_vec(self.bytes()?.to_vec()).into())
    }
    pub(crate) fn count(&mut self) -> CliResult<usize> {
        let count = self.u32()? as usize;
        if count > 16_384 {
            Err(CliError::Context("control collection length".to_owned()))
        } else {
            Ok(count)
        }
    }
    pub(crate) fn os_strings(&mut self) -> CliResult<Vec<std::ffi::OsString>> {
        use std::os::unix::ffi::OsStringExt;
        (0..self.count()?)
            .map(|_| Ok(std::ffi::OsString::from_vec(self.bytes()?.to_vec())))
            .collect()
    }
    pub(crate) fn done(self) -> CliResult<()> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(CliError::Context("control trailing bytes".to_owned()))
        }
    }
}

fn invocation_receipt(
    operation_id: OperationId,
    outcome: layerfs_sdk::OperationOutcome,
    total: std::time::Instant,
    parse_elapsed_ns: u64,
    context_open_elapsed_ns: u64,
    operation_wait_elapsed_ns: u64,
    render_elapsed_ns: u64,
) -> layerfs_sdk::CliInvocationReceipt {
    layerfs_sdk::CliInvocationReceipt {
        operation_id,
        outcome,
        total_elapsed_ns: elapsed_ns(total),
        parse_elapsed_ns,
        context_open_elapsed_ns,
        operation_wait_elapsed_ns,
        render_elapsed_ns,
    }
}

fn write_invocation(
    output: &mut impl std::io::Write,
    json: bool,
    receipt: layerfs_sdk::CliInvocationReceipt,
) -> CliResult<()> {
    if !receipt.timing_is_consistent() {
        return Err(CliError::Integrity);
    }
    if json {
        writeln!(output, "{}", receipt.to_json()).map_err(io)?;
    } else {
        writeln!(
            output,
            "INVOCATION operation={} outcome={:?} total_ns={} parse_ns={} context_ns={} wait_ns={} render_ns={}",
            receipt.operation_id,
            receipt.outcome,
            receipt.total_elapsed_ns,
            receipt.parse_elapsed_ns,
            receipt.context_open_elapsed_ns,
            receipt.operation_wait_elapsed_ns,
            receipt.render_elapsed_ns,
        )
        .map_err(io)?;
    }
    output.flush().map_err(io)
}

fn elapsed_ns(started: std::time::Instant) -> u64 {
    started.elapsed().as_nanos().try_into().unwrap_or(u64::MAX)
}

fn ensure_host(paths: &ContextPaths) -> CliResult<()> {
    if ping(paths) {
        return Ok(());
    }
    std::process::Command::new(host_binary()?)
        .arg("--host")
        .arg(&paths.context)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(io)?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        if ping(paths) {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    Err(CliError::Context("host READY timeout".to_owned()))
}

fn host_binary() -> CliResult<PathBuf> {
    let current = std::env::current_exe().map_err(io)?;
    if current.file_stem().is_some_and(|name| name == "layerfs") {
        return Ok(current);
    }
    let mut directory = current.parent();
    while let Some(value) = directory {
        let candidate = value.join("layerfs");
        if candidate.is_file() {
            return Ok(candidate);
        }
        if value.file_name().is_none_or(|name| name != "deps") {
            break;
        }
        directory = value.parent();
    }
    Err(CliError::Context("layerfs host executable".to_owned()))
}

fn ping(paths: &ContextPaths) -> bool {
    exchange(paths, vec![PING_REQUEST]).is_ok_and(|bytes| bytes == [PING_RESPONSE])
}

fn parse<T: std::str::FromStr>(value: &str, name: &str) -> CliResult<T> {
    value
        .parse()
        .map_err(|_| CliError::Context(name.to_owned()))
}

struct ConnectionGuard(Arc<std::sync::atomic::AtomicUsize>);

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, std::sync::atomic::Ordering::AcqRel);
    }
}

struct ContextLock {
    path: PathBuf,
    pid: PathBuf,
    socket: PathBuf,
    _file: std::fs::File,
}

impl ContextLock {
    fn acquire(paths: &ContextPaths) -> CliResult<Option<Self>> {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&paths.lock)
        {
            Ok(mut file) => {
                writeln!(file, "{}", std::process::id()).map_err(io)?;
                file.sync_all().map_err(io)?;
                Ok(Some(Self {
                    path: paths.lock.clone(),
                    pid: paths.pid.clone(),
                    socket: paths.socket.clone(),
                    _file: file,
                }))
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                if lock_is_live(paths)? {
                    return Ok(None);
                }
                validate_owned_regular(&paths.lock, &paths.runtime)?;
                std::fs::remove_file(&paths.lock).map_err(io)?;
                Self::acquire(paths)
            }
            Err(error) => Err(io(error)),
        }
    }
}

impl Drop for ContextLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket);
        let _ = std::fs::remove_file(&self.pid);
        let _ = std::fs::remove_file(&self.path);
    }
}

fn lock_is_live(paths: &ContextPaths) -> CliResult<bool> {
    let pid = std::fs::read_to_string(&paths.lock)
        .ok()
        .and_then(|value| value.trim().parse::<u32>().ok());
    Ok(pid.is_some_and(|pid| {
        std::process::Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "stat="])
            .output()
            .is_ok_and(|output| {
                output.status.success()
                    && String::from_utf8_lossy(&output.stdout)
                        .trim()
                        .chars()
                        .next()
                        .is_some_and(|state| state != 'Z')
            })
    }))
}

fn cleanup_socket(paths: &ContextPaths) -> CliResult<()> {
    use std::os::unix::fs::FileTypeExt;
    if paths.socket.exists() {
        let metadata = std::fs::symlink_metadata(&paths.socket).map_err(io)?;
        validate_owner(&metadata, &paths.runtime)?;
        if !metadata.file_type().is_socket() {
            return Err(CliError::Context("control socket type".to_owned()));
        }
        std::fs::remove_file(&paths.socket).map_err(io)?;
    }
    if paths.pid.exists() {
        validate_owned_regular(&paths.pid, &paths.runtime)?;
        std::fs::remove_file(&paths.pid).map_err(io)?;
    }
    Ok(())
}

fn validate_owned_regular(path: &Path, runtime: &Path) -> CliResult<()> {
    let metadata = std::fs::symlink_metadata(path).map_err(io)?;
    validate_owner(&metadata, runtime)?;
    if !metadata.file_type().is_file() {
        return Err(CliError::Context("runtime file type".to_owned()));
    }
    Ok(())
}

fn validate_owner(metadata: &std::fs::Metadata, runtime: &Path) -> CliResult<()> {
    use std::os::unix::fs::MetadataExt;
    let owner = std::fs::metadata(runtime).map_err(io)?.uid();
    if metadata.uid() == owner {
        Ok(())
    } else {
        Err(CliError::Context("runtime owner".to_owned()))
    }
}

#[cfg(target_os = "linux")]
fn peer_is_owner(stream: &std::os::unix::net::UnixStream, paths: &ContextPaths) -> CliResult<bool> {
    use std::os::unix::fs::MetadataExt;
    let owner = std::fs::metadata(&paths.runtime).map_err(io)?.uid();
    rustix::net::sockopt::get_socket_peercred(stream)
        .map(|credentials| credentials.uid.as_raw() == owner)
        .map_err(|error| CliError::Io(error.to_string()))
}

#[cfg(any(
    target_vendor = "apple",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "netbsd",
    target_os = "openbsd"
))]
fn peer_is_owner(stream: &std::os::unix::net::UnixStream, paths: &ContextPaths) -> CliResult<bool> {
    use std::os::unix::fs::MetadataExt;
    let owner = std::fs::metadata(&paths.runtime).map_err(io)?.uid();
    nix::unistd::getpeereid(stream)
        .map(|(uid, _)| uid.as_raw() == owner)
        .map_err(|error| CliError::Io(error.to_string()))
}

#[cfg(not(any(
    target_os = "linux",
    target_vendor = "apple",
    target_os = "freebsd",
    target_os = "dragonfly",
    target_os = "netbsd",
    target_os = "openbsd"
)))]
fn peer_is_owner(_: &std::os::unix::net::UnixStream, _: &ContextPaths) -> CliResult<bool> {
    Ok(true)
}

fn io(error: std::io::Error) -> CliError {
    CliError::Io(error.to_string())
}
