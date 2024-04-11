#![allow(dead_code, unused_imports)]
mod buffer;
mod command;
mod debug;
mod input;
mod prelude;
mod render;
mod term;
mod textobj;
mod tui;
mod window;
mod guile;
mod utils;
use prelude::*;

use libc::STDIN_FILENO;
use nix::sys::termios::{self, Termios};
use nix::sys::{
    signal::{self, SaFlags, SigHandler},
    signalfd::SigSet,
};
use render::Ctx;
use std::path::PathBuf;
use std::sync::Mutex;
use std::{
    panic::{self, PanicInfo},
    path::Path,
    sync::atomic::AtomicBool,
};

use crate::debug::log;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Normal,
    Insert,
    Command,
}

static EXIT_PENDING: AtomicBool = AtomicBool::new(false);
static DEFAULT_PANIC: std::sync::Mutex<
    Option<Box<dyn Fn(&PanicInfo<'_>) + 'static + Send + Sync>>,
> = std::sync::Mutex::new(None);

// holds the original termios state to restore to when exiting
static ORIGINAL_TERMIOS: Mutex<Option<Termios>> = Mutex::new(None);

fn exit() {
    EXIT_PENDING.store(true, std::sync::atomic::Ordering::Relaxed);
}

fn main_loop() {
    let path = if let Some(file) = std::env::args().nth(1) {
        PathBuf::from(file)
    } else {
        Path::new("./assets/test/passage_wrapped.txt").into()
    };

    let mut ctx: Ctx = Ctx::from_file(
        libc::STDIN_FILENO,
        &path
    )
    .unwrap();

    guile::initialize();

    ctx.render();
    let mut stdin = std::io::stdin().lock();
    loop {
        if let Some(token) = input::handle_input(&ctx, &mut stdin) {
            ctx.process_action(token);
            ctx.render();
        };
        if EXIT_PENDING.load(std::sync::atomic::Ordering::Acquire) {
            return;
        }
    }
}

fn main() -> Result<(), ()> {
    // panic handler is needed because we need to restore the terminal
    let mut guard = ORIGINAL_TERMIOS.lock().unwrap();
    *guard = Some(termios::tcgetattr(STDIN_FILENO).unwrap());
    *DEFAULT_PANIC.try_lock().expect("first thread to take lock") = Some(panic::take_hook());
    panic::set_hook(Box::new(panic_handler));
    drop(guard);

    // let buf = buffer::Buffer::new("./assets/test/passage_wrapped.txt").unwrap();
    // let buf = buffer::Buffer::new("./assets/test/crossbox.txt").unwrap();
    // let buf = buffer::Buffer::new_fromstring(String::new());
    // let buf = buffer::Buffer::new("./assets/test/lines.txt").unwrap();
    // let mut ctx = Ctx::from_buffer(libc::STDIN_FILENO, buf);


    main_loop();

    term::flush();
    term::altbuf_disable();
    println!();

    // eprintln!("reached end of main loop");
    if let Some(termios) =  ORIGINAL_TERMIOS.lock().unwrap().take()  {
        termios::tcsetattr(STDIN_FILENO, termios::SetArg::TCSANOW, &termios).unwrap_or(());
    } else {
        panic!("unable to reset terminal");
    }
    debug::cleanup();
    Ok(())
}

/// Panic handler. Needed becauase we take over the screen during execution and we should clean up
/// after ourselves.
fn panic_handler(pi: &PanicInfo) {
    eprint!("\n\n");

    if let Some(mut lock) = ORIGINAL_TERMIOS.lock().ok() {
        if let Some(termio) = lock.take()  {
            term::altbuf_disable();
            term::flush();
            termios::tcsetattr(STDIN_FILENO, termios::SetArg::TCSANOW, &termio).unwrap_or(());
        }
    }

    eprintln!("DON'T PANIC, it said in large, friendly letters.\n");

    debug::cleanup();

    if let Ok(mut lock) = DEFAULT_PANIC.try_lock() {
        if let Some(default_panic) = lock.take() {
            default_panic(pi);
        } else {
            eprintln!("default hook was not saved");
        }
    } else {
        eprintln!("unable to acquire lock on default panic hook");
    }
}
