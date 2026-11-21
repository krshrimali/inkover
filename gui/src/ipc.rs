//! Talks to the inkover daemon over its control socket, falling back to the CLI.
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

fn socket_path() -> PathBuf {
    let dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(dir).join("inkover.sock")
}

/// Send one command; `None` if the daemon isn't running.
pub fn send(args: &[&str]) -> Option<String> {
    let mut s = UnixStream::connect(socket_path()).ok()?;
    let t = Some(Duration::from_millis(400));
    s.set_read_timeout(t).ok()?;
    s.set_write_timeout(t).ok()?;
    s.write_all(format!("{}\n", args.join(" ")).as_bytes()).ok()?;
    let mut out = String::new();
    s.read_to_string(&mut out).ok()?;
    Some(out.trim().to_string())
}

/// Path to the `inkover` CLI: $INKOVER_BIN, this checkout's bin/, ~/.local/bin, then $PATH.
pub fn cli_path() -> PathBuf {
    if let Ok(p) = std::env::var("INKOVER_BIN") {
        return p.into();
    }
    if let Ok(exe) = std::env::current_exe().and_then(|p| p.canonicalize()) {
        // gui/target/release/inkover-gui -> bin/inkover
        if let Some(repo) = exe.ancestors().nth(4) {
            let cand = repo.join("bin/inkover");
            if cand.exists() {
                return cand;
            }
        }
    }
    let home = std::env::var("HOME").unwrap_or_default();
    let local = PathBuf::from(home).join(".local/bin/inkover");
    if local.exists() { local } else { "inkover".into() }
}

/// Run `inkover <args>` in a background thread (it may wait for the daemon to start).
pub fn run_cli(args: &[&str]) {
    let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    std::thread::spawn(move || {
        let _ = Command::new(cli_path())
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    });
}

/// Socket if running, else the CLI (which auto-starts the daemon for start-ish commands).
pub fn command(args: &[&str]) -> Option<String> {
    match send(args) {
        Some(r) => Some(r),
        None => {
            run_cli(args);
            None
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Status {
    pub draw_mode: bool,
    pub tool: String,
    pub color: usize, // 1-based
    pub size: f64,
    pub recording: Option<u64>,
    pub recording_path: Option<String>,
    pub board: u64,
    pub toolbar_visible: bool,
    pub palette: Vec<String>,
    pub monitors: Vec<(String, u64)>,
}

pub fn status() -> Option<Status> {
    let raw = send(&["status"])?;
    let v: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let mut monitors: Vec<(String, u64)> = v["monitors"]
        .as_object()
        .map(|m| m.iter().map(|(k, n)| (k.clone(), n.as_u64().unwrap_or(0))).collect())
        .unwrap_or_default();
    monitors.sort();
    Some(Status {
        draw_mode: v["mode"] == "draw",
        tool: v["tool"].as_str().unwrap_or("pen").to_string(),
        color: v["color"].as_u64().unwrap_or(1) as usize,
        size: v["size"].as_f64().unwrap_or(5.0),
        recording: v["recording"].as_u64(),
        recording_path: v["recording_path"].as_str().map(String::from),
        board: v["board"].as_u64().unwrap_or(0),
        toolbar_visible: v["toolbar_visible"].as_bool().unwrap_or(true),
        palette: v["palette"]
            .as_array()
            .map(|a| a.iter().filter_map(|c| c.as_str().map(String::from)).collect())
            .unwrap_or_default(),
        monitors,
    })
}
