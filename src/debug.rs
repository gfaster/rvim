use core::fmt;
use std::fs;
use std::io::Write;
use std::os::unix::fs::FileTypeExt;
use std::path::Path;
use std::process::{Child, Command};
use std::sync::{Mutex, MutexGuard};

use nix::unistd::mkfifo;

#[allow(unused)]
macro_rules! log {
    ($($arg:tt)*) => {{
        $crate::debug::log_args(std::format_args!("{}\n", std::format_args!($($arg)*)));
    }};
}
pub(crate) use log;

struct LogComponents {
    child: Child,
    file: std::fs::File,
}

static OUTPUT: Mutex<Option<LogComponents>> = Mutex::new(None);
const LOG_FILE: &str = "./rvim.log";

fn init_log() -> MutexGuard<'static, Option<LogComponents>> {
    let mut guard = OUTPUT.lock().unwrap();
    if guard.is_some() {
        return guard;
    }

    let mut file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .open(LOG_FILE)
        .expect("logfile created");

    file.set_len(0).unwrap();
    file.write(&format!("New log: \n").as_bytes()).unwrap();
    file.flush().unwrap();

    // if the file load fails, then we have no way of knowing - alacritty will display a popup
    // error instead of returning a failure exit code
    let term = std::env::var("TERM").unwrap_or("xterm".to_owned());
    let child = Command::new(&term)
        .arg("--command")
        .arg("tail")
        .arg("-f")
        .arg(LOG_FILE.escape_debug().to_string())
        .spawn()
        .unwrap();

    *guard = Some(LogComponents { child, file });

    guard
}

pub fn is_init() -> bool {
    OUTPUT.try_lock().ok().map(|x| x.is_some()).unwrap_or(false)
}

pub fn cleanup() {
    if is_init() {
    }
}

pub fn log_args(args: fmt::Arguments) {
    if cfg!(test) {
        eprintln!("{}", args);
        return;
    } 
    if !cfg!(debug_assertions) {
        // change to log file for release builds
        eprintln!("{}", args);
        return;
    }

    let mut guard = init_log();
    guard
        .as_mut()
        .expect("log initialized")
        .file
        .write_fmt(args)
        // .expect("write succeeds")
        .unwrap_or(());
}

pub fn sleep(seconds: u64) {
    std::thread::sleep(std::time::Duration::from_secs(seconds))
}
