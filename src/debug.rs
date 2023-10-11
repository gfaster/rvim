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
const OUTPUT_FIFO_PATH: &str = "/tmp/rvim_log.fifo";

fn init_log() -> MutexGuard<'static, Option<LogComponents>> {
    let mut guard = OUTPUT.lock().unwrap();
    if guard.is_some() {
        assert!(fs::metadata(OUTPUT_FIFO_PATH)
            .unwrap()
            .file_type()
            .is_fifo());
        return guard;
    }

    if !Path::new(OUTPUT_FIFO_PATH).exists() {
        mkfifo(
            OUTPUT_FIFO_PATH,
            nix::sys::stat::Mode::S_IRUSR | nix::sys::stat::Mode::S_IWUSR,
        )
        .unwrap();
    }

    // if the file load fails, then we have no way of knowing - alacritty will display a popup
    // error instead of returning a failure exit code
    let child = Command::new("/usr/bin/alacritty")
        .arg("--command")
        .arg("/bin/cat")
        .arg(OUTPUT_FIFO_PATH.escape_debug().to_string())
        .spawn()
        .unwrap();
    let file = fs::OpenOptions::new()
        .write(true)
        .open(OUTPUT_FIFO_PATH)
        .expect("fifo created");
    assert!(fs::metadata(OUTPUT_FIFO_PATH)
        .unwrap()
        .file_type()
        .is_fifo());

    *guard = Some(LogComponents { child, file });

    guard
}

pub fn is_init() -> bool {
    OUTPUT.try_lock().ok().map(|x| x.is_some()).unwrap_or(false)
}

pub fn cleanup() {
    if is_init() {
        fs::remove_file(OUTPUT_FIFO_PATH)
            .unwrap_or_else(|_| eprintln!("failed to delete fifo for log"));
    }
}

pub fn log_args(args: fmt::Arguments) {
    if cfg!(test) {
        eprintln!("{}", args);
        return;
    } else if !cfg!(debug_assertions) {
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
