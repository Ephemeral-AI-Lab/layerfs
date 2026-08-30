#[cfg(unix)]
mod unix {
    use std::ffi::OsString;
    use std::io::{self, Read, Write};
    use std::os::unix::fs::FileTypeExt;
    use std::os::unix::net::UnixStream;
    use std::path::Path;

    const SOCKET: &str = "/var/run/docker.sock";
    const API: &str = "/v1.41";

    pub(crate) struct DockerExec {
        container: String,
        id: String,
        stream: Option<UnixStream>,
        pid_file: Option<String>,
    }

    impl DockerExec {
        pub(crate) fn available() -> bool {
            std::fs::metadata(SOCKET).is_ok_and(|metadata| metadata.file_type().is_socket())
        }

        pub(crate) fn start(container: &str, root: &Path, argv: &[OsString]) -> io::Result<Self> {
            if !safe_id(container) || argv.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Docker Exec input",
                ));
            }
            let argv = argv
                .iter()
                .map(|value| {
                    value.to_str().map(json_string).ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidInput, "Docker Exec argv")
                    })
                })
                .collect::<io::Result<Vec<_>>>()?
                .join(",");
            let root = root
                .to_str()
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Docker Exec root"))?;
            let body = format!(
                "{{\"AttachStdin\":false,\"AttachStdout\":true,\"AttachStderr\":true,\"Tty\":false,\"WorkingDir\":{},\"Cmd\":[{}]}}",
                json_string(root), argv
            );
            let (_, mut response) = request(
                "POST",
                &format!("{API}/containers/{container}/exec"),
                &body,
                false,
            )?;
            let mut response_body = String::new();
            response.read_to_string(&mut response_body)?;
            let id = string_field(&response_body, "Id")?.to_owned();
            if !safe_id(&id) {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "Docker Exec id"));
            }
            let (_, stream) = request(
                "POST",
                &format!("{API}/exec/{id}/start"),
                "{\"Detach\":false,\"Tty\":false}",
                true,
            )?;
            Ok(Self {
                container: container.to_owned(),
                id,
                stream: Some(stream),
                pid_file: None,
            })
        }

        pub(crate) fn start_wrapped(
            container: &str,
            root: &Path,
            argv: &[OsString],
            pid_file: &str,
        ) -> io::Result<Self> {
            let mut wrapped = vec![
                OsString::from("/bin/sh"),
                OsString::from("-c"),
                OsString::from(
                    "pid_file=$1; shift; pgid=$(cut -d ' ' -f 5 \"/proc/$$/stat\") || exit 125; [ \"$pgid\" = \"$$\" ] || exit 125; (umask 077 && printf '%s\\n' \"$$\" > \"$pid_file\") || exit 125; trap 'rm -f \"$pid_file\"' EXIT; \"$@\"",
                ),
                OsString::from("layerfs-exec"),
                OsString::from(pid_file),
            ];
            wrapped.extend_from_slice(argv);
            let mut execution = Self::start(container, root, &wrapped)?;
            execution.pid_file = Some(pid_file.to_owned());
            Ok(execution)
        }

        pub(crate) fn take_stream(&mut self) -> io::Result<UnixStream> {
            self.stream
                .take()
                .ok_or_else(|| io::Error::other("Docker Exec stream already taken"))
        }

        pub(crate) fn exit_code(&self) -> io::Result<Option<i32>> {
            let state = inspect(&self.id)?;
            if state.running {
                return Err(io::Error::other("Docker Exec stream ended while running"));
            }
            Ok(state.exit_code)
        }

        pub(crate) fn stop(&self) -> io::Result<bool> {
            let state = inspect(&self.id)?;
            if !state.running {
                return Ok(false);
            }
            let pid_file = self
                .pid_file
                .as_deref()
                .ok_or_else(|| io::Error::other("Docker Exec PID file"))?;
            let script = "attempts=0; while [ ! -s \"$1\" ]; do attempts=$((attempts + 1)); [ \"$attempts\" -lt 100 ] || exit 2; sleep 0.01; done; group=$(cat \"$1\") || exit 1; if kill -TERM -\"$group\" 2>/dev/null; then exit 0; fi; if kill -0 -\"$group\" 2>/dev/null; then exit 1; fi; exit 2";
            let mut signal = Self::start(
                &self.container,
                Path::new("/"),
                &[
                    OsString::from("/bin/sh"),
                    OsString::from("-c"),
                    OsString::from(script),
                    OsString::from("layerfs-stop"),
                    OsString::from(pid_file),
                ],
            )?;
            drain_multiplexed(&mut signal.take_stream()?, |_, _| Ok(()))?;
            if signal.exit_code()? == Some(0) {
                Ok(true)
            } else {
                Err(io::Error::other("Docker Exec process-group signal"))
            }
        }
    }

    pub(crate) fn drain_multiplexed(
        stream: &mut UnixStream,
        mut output: impl FnMut(u8, &[u8]) -> io::Result<()>,
    ) -> io::Result<()> {
        let mut header = [0_u8; 8];
        loop {
            let read = stream.read(&mut header[..1])?;
            if read == 0 {
                return Ok(());
            }
            stream.read_exact(&mut header[1..])?;
            let length =
                u32::from_be_bytes(header[4..8].try_into().expect("frame length")) as usize;
            let mut bytes = vec![0_u8; length];
            stream.read_exact(&mut bytes)?;
            output(header[0], &bytes)?;
        }
    }

    struct Inspect {
        running: bool,
        exit_code: Option<i32>,
    }

    fn inspect(id: &str) -> io::Result<Inspect> {
        let (_, mut response) = request("GET", &format!("{API}/exec/{id}/json"), "", false)?;
        let mut body = String::new();
        response.read_to_string(&mut body)?;
        Ok(Inspect {
            running: bool_field(&body, "Running")?,
            exit_code: integer_field(&body, "ExitCode")?.map(|value| value as i32),
        })
    }

    fn request(
        method: &str,
        path: &str,
        body: &str,
        upgrade: bool,
    ) -> io::Result<(u16, UnixStream)> {
        let mut stream = UnixStream::connect(SOCKET)?;
        write!(
            stream,
            "{method} {path} HTTP/1.1\r\nHost: docker\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{}\r\n{body}",
            body.len(),
            if upgrade {
                "Connection: Upgrade\r\nUpgrade: tcp\r\n"
            } else {
                "Connection: close\r\n"
            }
        )?;
        stream.flush()?;
        let mut headers = Vec::with_capacity(512);
        while !headers.ends_with(b"\r\n\r\n") {
            if headers.len() == 64 * 1024 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Docker HTTP headers",
                ));
            }
            let mut byte = [0_u8; 1];
            stream.read_exact(&mut byte)?;
            headers.push(byte[0]);
        }
        let status = std::str::from_utf8(&headers)
            .ok()
            .and_then(|headers| headers.lines().next())
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|status| status.parse::<u16>().ok())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Docker HTTP status"))?;
        if status != 101 && !(200..300).contains(&status) {
            let mut message = String::new();
            stream.read_to_string(&mut message)?;
            return Err(io::Error::other(format!("Docker HTTP {status}: {message}")));
        }
        Ok((status, stream))
    }

    fn string_field<'a>(body: &'a str, key: &str) -> io::Result<&'a str> {
        let value = field(body, key)?;
        value
            .strip_prefix('"')
            .and_then(|value| value.split_once('"').map(|(value, _)| value))
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Docker JSON string"))
    }

    fn bool_field(body: &str, key: &str) -> io::Result<bool> {
        match field(body, key)? {
            value if value.starts_with("true") => Ok(true),
            value if value.starts_with("false") => Ok(false),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Docker JSON bool",
            )),
        }
    }

    fn integer_field(body: &str, key: &str) -> io::Result<Option<i64>> {
        let value = field(body, key)?;
        if value.starts_with("null") {
            return Ok(None);
        }
        let value = value
            .split(|byte: char| !byte.is_ascii_digit() && byte != '-')
            .next()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Docker JSON integer"))?;
        value
            .parse()
            .map(Some)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Docker JSON integer"))
    }

    fn field<'a>(body: &'a str, key: &str) -> io::Result<&'a str> {
        let key = format!("\"{key}\"");
        let value = body
            .find(&key)
            .and_then(|index| {
                body[index + key.len()..]
                    .split_once(':')
                    .map(|(_, value)| value)
            })
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Docker JSON field"))?;
        Ok(value.trim_start())
    }

    fn json_string(value: &str) -> String {
        let mut output = String::with_capacity(value.len() + 2);
        output.push('"');
        for character in value.chars() {
            match character {
                '"' => output.push_str("\\\""),
                '\\' => output.push_str("\\\\"),
                '\n' => output.push_str("\\n"),
                '\r' => output.push_str("\\r"),
                '\t' => output.push_str("\\t"),
                character if character < ' ' => {
                    use std::fmt::Write as _;
                    let _ = write!(output, "\\u{:04x}", character as u32);
                }
                character => output.push(character),
            }
        }
        output.push('"');
        output
    }

    fn safe_id(value: &str) -> bool {
        !value.is_empty()
            && value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    }
}

#[cfg(unix)]
pub(crate) use unix::{drain_multiplexed, DockerExec};
