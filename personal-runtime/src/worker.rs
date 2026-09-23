//! Durable foreground-command worker. It never initializes the desktop or its tunnels.
use std::{
    fs,
    io::{Read, Write},
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use serde_json::json;
use crate::{now_ms, Error, Result, Store};
use crate::jobs::{JobSpec, MAX_HEAVY, MAX_RUNNING, MAX_STREAM_BYTES};
use crate::locks::{self, Guard};

pub fn run_from_args() -> Option<i32> {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) != Some("--personal-job-worker") { return None; }
    if args.len() != 4 { return Some(2); }
    match run(PathBuf::from(&args[2]), &args[3]) {
        Ok(()) => Some(0),
        Err(error) => { eprintln!("personal worker: {error}"); Some(1) }
    }
}

pub fn run(dir: PathBuf, job: &str) -> Result<()> {
    crate::store::id(job)?;
    let connection = rusqlite::Connection::open_with_flags(
        dir.join("runtime.sqlite3"), rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )?;
    connection.busy_timeout(Duration::from_secs(10))?;
    let raw: String = connection.query_row("SELECT spec FROM jobs WHERE id=?1", [job], |row| row.get(0))?;
    let spec: JobSpec = serde_json::from_str(&raw)?;
    drop(connection);
    let store = Store::at(dir, spec.workspace.clone())?;
    spec.validate(&store.workspace)?;
    // Not inherited by the command: its absence is evidence that supervision was lost.
    let Some(_alive) = locks::try_gate(&store.dir, &format!("alive:{job}"), true)? else { return Ok(()); };
    let state: String = store.conn()?.query_row("SELECT state FROM jobs WHERE id=?1", [job], |row| row.get(0))?;
    if state != "queued" { return Ok(()); }
    let logdir = store.dir.join("jobs").join(job);
    crate::store::private_dir(&logdir)?;
    write_json(&logdir.join("worker.json"), &json!({"job_id":job,"worker_pid":std::process::id(),"created":now_ms()}))?;
    let start = Instant::now();
    let mut heartbeat = Instant::now();
    let control = store.conn()?;
    let mut delay = 20u64;
    let jitter = job.bytes().fold(0u64, |sum, byte| sum + byte as u64) % 17;
    let mut waiting_reported = false;
    let guards = loop {
        if store.job_cancelled_on(&control,job)? {
            store.finish_job(job, "cancelled", None, "cancelled before execution")?;
            return Ok(());
        }
        if start.elapsed() >= Duration::from_secs(1800) {
            store.finish_job(job, "queue_timeout", None, "queue exceeded 30 minutes; command not launched")?;
            return Ok(());
        }
        let waiting = match acquire(&store, &spec)? {
            Admission::Ready(guards) => break guards,
            Admission::Waiting(reason) => reason,
        };
        if !waiting_reported || heartbeat.elapsed() >= Duration::from_secs(1) {
            control.execute("UPDATE jobs SET updated=?2,detail=?3 WHERE id=?1 AND state='queued'", (job, now_ms(), format!("waiting:{waiting}")))?;
            heartbeat = Instant::now();
            waiting_reported = true;
        }
        // Desynchronize contenders; queue sleep is capped at 266ms. Database
        // contention can add latency. Failed tries drop every partial reservation.
        std::thread::sleep(Duration::from_millis(delay + jitter));
        delay = (delay * 2).min(250);
    };
    let claimed = control.execute(
        "UPDATE jobs SET state='running',detail='',updated=?2 WHERE id=?1 AND state='queued'", (job, now_ms()),
    )?;
    if claimed != 1 { return Ok(()); }
    drop(control);
    let result = execute(&store, job, &spec, guards);
    // execute's owned-child guard runs before resource locks can be released.
    if let Err(error) = &result {
        let _ = store.finish_job(job, "unknown", None, &format!("worker error {}; inspect effects before retry", error.code()));
    }
    result
}

enum Admission { Ready(Vec<Guard>), Waiting(&'static str) }

fn acquire(store: &Store, spec: &JobSpec) -> Result<Admission> {
    let mut guards = Vec::new();
    if spec.mode != "read" {
        let Some(guard) = locks::try_gate(&store.dir, "source", spec.mode == "write")? else { return Ok(Admission::Waiting("source")); };
        guards.push(guard);
        // An unknown historical result must not permanently occupy the whole
        // workspace. Active cooperative children retain the source/resource locks;
        // after those release, independent new work can proceed. The original job
        // stays unknown and its request_id is never automatically executed again.
    }
    let mut resources = spec.resources.clone();
    resources.sort(); resources.dedup();
    for resource in resources {
        let Some(guard) = locks::try_gate(&store.dir, &format!("resource:{resource}"), true)? else { return Ok(Admission::Waiting("resource")); };
        guards.push(guard);
    }
    if spec.mode == "build" {
        let Some(guard) = locks::slot(&store.dir, "heavy", MAX_HEAVY)? else { return Ok(Admission::Waiting("heavy_capacity")); };
        guards.push(guard);
    }
    let Some(guard) = locks::slot(&store.dir, "commands", MAX_RUNNING)? else { return Ok(Admission::Waiting("command_capacity")); };
    guards.push(guard);
    Ok(Admission::Ready(guards))
}

struct OwnedCommand { child: Child, group_cleaned: bool }
impl OwnedCommand {
    fn stop(&mut self) {
        if self.group_cleaned { return; }
        #[cfg(unix)] unsafe { libc::kill(-(self.child.id() as i32), libc::SIGKILL); }
        #[cfg(not(unix))] { let _ = self.child.kill(); }
        let _ = self.child.wait();
        self.group_cleaned = true;
    }
}
impl Drop for OwnedCommand { fn drop(&mut self) { self.stop(); } }

fn execute(store: &Store, job: &str, spec: &JobSpec, guards: Vec<Guard>) -> Result<()> {
    let logdir = store.dir.join("jobs").join(job);
    let mut command = Command::new(&spec.program);
    command.args(&spec.args).current_dir(&spec.cwd)
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(unix)] {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
        // If the supervisor is killed, cooperative foreground children retain their
        // resource slots. A lost supervisor becomes unknown, never automatically replayed.
        let fds: Vec<_> = guards.iter().map(Guard::raw_fd).collect();
        unsafe {
            command.pre_exec(move || {
                for fd in &fds {
                    let flags = libc::fcntl(*fd, libc::F_GETFD);
                    if flags < 0 || libc::fcntl(*fd, libc::F_SETFD, flags & !libc::FD_CLOEXEC) < 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                }
                Ok(())
            });
        }
    }
    #[cfg(windows)] {
        use std::os::windows::process::CommandExt;
        let _ = guards;
        command.creation_flags(0x08000000 | 0x00000200);
        command.env("PYTHONUTF8", "1").env("PYTHONIOENCODING", "utf-8");
    }
    let child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            // No command was started; release reservations before publishing.
            drop(guards);
            store.finish_job(job, "spawn_failed", None, &format!("command launch failed: {}", error.kind()))?;
            return Ok(());
        }
    };
    let mut owned = OwnedCommand { child, group_cleaned: false };
    write_json(&logdir.join("child.json"), &json!({"job_id":job,"child_pid":owned.child.id(),"started":now_ms()}))?;
    let stdout = owned.child.stdout.take().ok_or_else(|| Error::contract("PIPE_MISSING", "stdout unavailable"))?;
    let stderr = owned.child.stderr.take().ok_or_else(|| Error::contract("PIPE_MISSING", "stderr unavailable"))?;
    let out_reader = std::thread::spawn({ let dir = logdir.clone(); move || capture(stdout, dir, "stdout") });
    let err_reader = std::thread::spawn({ let dir = logdir.clone(); move || capture(stderr, dir, "stderr") });
    // Writing stdin cannot block timeout/cancellation supervision. Empty input also closes it.
    let input = spec.stdin.clone();
    let stdin = owned.child.stdin.take();
    let input_writer = std::thread::spawn(move || { if let Some(mut stdin) = stdin { let _ = stdin.write_all(input.as_bytes()); } });
    let start = Instant::now();
    let outcome = monitor(store, job, &mut owned, Duration::from_millis(spec.timeout_ms));
    owned.stop(); // Also closes pipes inherited by foreground descendants.
    let _ = input_writer.join();
    let stdout_result = out_reader.join().map_err(|_| Error::contract("READER_PANIC", "stdout collector failed"))?;
    let stderr_result = err_reader.join().map_err(|_| Error::contract("READER_PANIC", "stderr collector failed"))?;
    let (state, exit_code) = outcome?;
    write_json(&logdir.join("output.json"), &json!({
        "stdout":stdout_result?,"stderr":stderr_result?,"duration_ms":start.elapsed().as_millis()
    }))?;
    // Child groups have stopped, pipes are joined and output is durable. A
    // terminal receipt must not race with still-held source/resource permits.
    // On earlier errors, parameter drop runs after OwnedCommand's cleanup.
    drop(guards);
    store.finish_job(job, state, exit_code, if state == "exited" {
        "command exited; business acceptance is separate"
    } else { "owned command stopped; inspect side effects before retry" })?;
    if let Ok(Some(task)) = store.conn()?.query_row("SELECT task_id FROM jobs WHERE id=?1", [job], |row| row.get::<_,Option<String>>(0)) {
        // The durable job outcome remains authoritative even if an optional event fails.
        let _ = store.event(&task, "job_finished", &json!({"job_id":job,"state":state,"exit_code":exit_code}));
    }
    Ok(())
}

fn monitor(store: &Store, job: &str, owned: &mut OwnedCommand, timeout: Duration) -> Result<(&'static str, Option<i32>)> {
    let start = Instant::now();
    let mut heartbeat = Instant::now();
    let control = store.conn()?;
    loop {
        if let Some(status) = owned.child.try_wait()? { return Ok(("exited", status.code())); }
        if store.job_cancelled_on(&control,job)? { return Ok(("cancelled", None)); }
        if start.elapsed() >= timeout { return Ok(("timeout", None)); }
        if heartbeat.elapsed() >= Duration::from_secs(1) {
            control.execute("UPDATE jobs SET updated=?2 WHERE id=?1 AND state='running'", (job, now_ms()))?;
            heartbeat = Instant::now();
        }
        std::thread::sleep(Duration::from_millis(30));
    }
}

fn write_json(path: &std::path::Path, value: &serde_json::Value) -> Result<()> {
    let mut file = fs::File::create(path)?;
    file.write_all(value.to_string().as_bytes())?;
    file.sync_all()?;
    Ok(())
}

fn capture(mut input: impl Read, dir: PathBuf, name: &str) -> Result<serde_json::Value> {
    let mut file = fs::OpenOptions::new().write(true).create_new(true).open(dir.join(format!("{name}.log")))?;
    let (mut total, mut kept) = (0usize, 0usize);
    let mut tail = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 { break; }
        total = total.saturating_add(count);
        let take = count.min(MAX_STREAM_BYTES.saturating_sub(kept));
        if take > 0 { file.write_all(&buffer[..take])?; kept += take; }
        tail.extend_from_slice(&buffer[..count]);
        if tail.len() > 65536 { tail.drain(..tail.len()-65536); }
    }
    file.sync_all()?;
    let mut tail_file = fs::File::create(dir.join(format!("{name}.tail")))?;
    tail_file.write_all(&tail)?; tail_file.sync_all()?;
    Ok(json!({"total_bytes":total,"retained_bytes":kept,"truncated":total>kept,"tail_bytes":tail.len()}))
}
