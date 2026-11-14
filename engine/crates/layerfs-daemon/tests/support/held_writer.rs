//! External process holding the real Store write ownership, with bounded life.
use std::{
    io::{BufRead, BufReader, Write},
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::Duration,
};

pub struct HeldWriter(Child);
impl HeldWriter {
    pub fn acquire(database: &std::path::Path) -> Self {
        // A blocking readline waits for the release line on every host;
        // select() on stdin is a unix-only shape (Windows: sockets only).
        let script = "import sqlite3,sys\nc=sqlite3.connect(sys.argv[1],timeout=0)\nc.execute('BEGIN IMMEDIATE')\nprint('held',flush=True)\nsys.stdin.readline()\nc.rollback()\nc.close()\n";
        #[cfg(windows)]
        let interpreter = "python";
        #[cfg(not(windows))]
        let interpreter = "python3";
        let child = Command::new(interpreter)
            .arg("-c")
            .arg(script)
            .arg(database)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut held = Self(child);
        let mut output = BufReader::new(held.0.stdout.take().unwrap());
        let (tx, rx) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            let mut line = String::new();
            let result = output.read_line(&mut line).map(|_| line);
            let _ = tx.send(result);
        });
        let answer = rx.recv_timeout(Duration::from_secs(3)).unwrap().unwrap();
        assert_eq!(answer.trim_end(), "held", "the holder answered: {answer:?}");
        reader.join().unwrap();
        held
    }
    pub fn release(mut self) {
        self.0
            .stdin
            .take()
            .unwrap()
            .write_all(b"release\n")
            .unwrap();
        assert!(self.0.wait().unwrap().success());
    }
}
impl Drop for HeldWriter {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(None)) {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}
