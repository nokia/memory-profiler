use std::ffi::OsStr;
use std::fs;
use std::io::{self, BufRead, BufReader, Read};
use std::mem;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub static EMPTY_ARGS: &[&str] = &[];
pub static EMPTY_ENV: &[(&str, &str)] = &[];

fn run_internal<'a, R, I, N, C, E, S, P, Q, F>(
    cwd: C,
    executable: E,
    args: I,
    envs: N,
    callback: F,
) -> R
where
    I: IntoIterator<Item = S>,
    N: IntoIterator<Item = &'a (P, Q)>,
    C: AsRef<Path>,
    E: AsRef<OsStr>,
    S: AsRef<OsStr>,
    P: AsRef<OsStr> + 'a,
    Q: AsRef<OsStr> + 'a,
    F: FnOnce(Command) -> Result<R, io::Error>,
{
    let executable = executable.as_ref();
    let args: Vec<_> = args
        .into_iter()
        .map(|arg| arg.as_ref().to_owned())
        .collect();

    let mut invocation: String = executable.to_string_lossy().into_owned();
    for arg in &args {
        invocation.push_str(" ");
        invocation.push_str(&arg.to_string_lossy());
    }

    eprintln!("> {}", invocation);

    let mut cmd = Command::new(executable);
    cmd.args(args);
    cmd.current_dir(cwd);

    for (key, value) in envs.into_iter() {
        cmd.env(key, value);
    }

    match callback(cmd) {
        Ok(value) => value,
        Err(error) => {
            panic!(
                "Failed to launch `{}`: {:?}",
                executable.to_string_lossy(),
                error
            );
        }
    }
}

#[must_use]
pub struct CommandResult {
    status: ExitStatus,
    output: String,
}

impl CommandResult {
    pub fn assert_success(self) {
        if !self.status.success() {
            panic!("Command exited with a status of {:?}!", self.status.code());
        }
    }

    #[allow(dead_code)]
    pub fn assert_failure(self) {
        if self.status.success() {
            panic!("Command unexpectedly succeeded!");
        }
    }

    #[allow(dead_code)]
    pub fn output(&self) -> &str {
        &self.output
    }
}

fn print_stream<T: Read + Send + 'static>(
    fp: T,
    output: Arc<Mutex<String>>,
) -> thread::JoinHandle<()> {
    let fp = BufReader::new(fp);
    thread::spawn(move || {
        for line in fp.lines() {
            let line = match line {
                Ok(line) => line,
                Err(_) => break,
            };

            let mut output = output.lock().unwrap();
            output.push_str(&line);
            output.push_str("\n");
        }
    })
}

pub struct ChildHandle {
    output: Arc<Mutex<String>>,
    stdout_join: Option<thread::JoinHandle<()>>,
    stderr_join: Option<thread::JoinHandle<()>>,
    child: Child,
}

impl ChildHandle {
    pub fn wait(mut self) -> CommandResult {
        let start = Instant::now();
        let mut status = None;
        while start.elapsed() < Duration::from_secs(60) {
            status = self.child.try_wait().unwrap();
            if status.is_some() {
                break;
            }
        }

        let status = match status {
            Some(status) => status,
            None => {
                panic!("Timeout while waiting for the child process to exit!");
            }
        };

        let output = self.flush_output();

        CommandResult { status, output }
    }

    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    fn flush_output(&mut self) -> String {
        if let Some(stdout_join) = self.stdout_join.take() {
            let _ = stdout_join.join();
        }

        if let Some(stderr_join) = self.stderr_join.take() {
            let _ = stderr_join.join();
        }

        let mut output = String::new();
        mem::swap(&mut output, &mut self.output.lock().unwrap());
        print!("{}", output);

        output
    }
}

impl Drop for ChildHandle {
    fn drop(&mut self) {
        let _ = self.child.kill();
        self.flush_output();
    }
}

pub fn run_in_the_background<C, E, S, P, Q>(
    cwd: C,
    executable: E,
    args: &[S],
    envs: &[(P, Q)],
) -> ChildHandle
where
    C: AsRef<Path>,
    E: AsRef<OsStr>,
    S: AsRef<OsStr>,
    P: AsRef<OsStr>,
    Q: AsRef<OsStr>,
{
    run_internal(cwd, executable, args, envs, |mut cmd| {
        let output = Arc::new(Mutex::new(String::new()));
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd.spawn()?;
        let stdout_join = print_stream(child.stdout.take().unwrap(), output.clone());
        let stderr_join = print_stream(child.stderr.take().unwrap(), output.clone());

        Ok(ChildHandle {
            output,
            stdout_join: Some(stdout_join),
            stderr_join: Some(stderr_join),
            child,
        })
    })
}

pub fn run<C, E, S, P, Q>(cwd: C, executable: E, args: &[S], envs: &[(P, Q)]) -> CommandResult
where
    C: AsRef<Path>,
    E: AsRef<OsStr>,
    S: AsRef<OsStr>,
    P: AsRef<OsStr>,
    Q: AsRef<OsStr>,
{
    run_in_the_background(cwd, executable, args, envs).wait()
}

pub fn assert_file_exists<P: AsRef<Path>>(path: P) {
    let path = path.as_ref();
    if !path.exists() {
        panic!("File {:?} doesn't exist", path);
    }
}

pub fn assert_file_missing<P: AsRef<Path>>(path: P) {
    let path = path.as_ref();
    if path.exists() {
        panic!("File {:?} exists", path);
    }
}

pub fn dir_entries<P: AsRef<Path>>(path: P) -> Result<Vec<PathBuf>, io::Error> {
    let mut output = Vec::new();
    let path = path.as_ref();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        output.push(entry.path());
    }

    Ok(output)
}
