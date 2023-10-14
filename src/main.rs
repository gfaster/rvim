#![allow(dead_code, unused_imports)]
mod buffer;
mod command;
mod debug;
mod input;
mod prelude;
mod render;
mod term;
mod textobj;
mod window;
use prelude::*;

use libc::STDIN_FILENO;
use nix::sys::termios::{self, Termios};
use nix::sys::{
    signal::{self, SaFlags, SigHandler},
    signalfd::SigSet,
};
use render::Ctx;
use std::{
    panic::{self, PanicInfo},
    path::Path,
    sync::atomic::AtomicBool,
};

#[allow(unused_imports)]
use crate::debug::log;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Normal,
    Insert,
    Command,
}

static FALLBACK_EXIT: AtomicBool = AtomicBool::new(false);
static EXIT_CHANNEL: std::sync::Mutex<Option<tokio::sync::oneshot::Sender<()>>> = std::sync::Mutex::new(None);

static DEFAULT_PANIC: std::sync::Mutex<
    Option<Box<dyn Fn(&PanicInfo<'_>) + 'static + Send + Sync>>,
> = std::sync::Mutex::new(None);

// holds the original termios state to restore to when exiting
static mut ORIGINAL_TERMIOS: Option<Termios> = None;

fn exit() {
    FALLBACK_EXIT.store(true, std::sync::atomic::Ordering::Relaxed);
    if let Some(ch) = EXIT_CHANNEL.lock().unwrap().take() {
        let _ = ch.send(());
    }
}

async fn main_loop() {
    let mut ctx: Ctx = Ctx::from_file(
        libc::STDIN_FILENO,
        Path::new("./assets/test/passage_wrapped.txt"),
    )
    .unwrap();
    ctx.render();
    let mut stdin = tokio::io::stdin();
    loop {
        tokio::select! {
            Some(token) = input::handle_input(&ctx, &mut stdin) => {
                ctx.process_action(token);
                ctx.render();
            },
        };
    }
}

fn main() -> Result<(), ()> {

    // panic handler is needed because we need to restore the terminal
    unsafe {
        ORIGINAL_TERMIOS = Some(termios::tcgetattr(STDIN_FILENO).unwrap());
    }
    *DEFAULT_PANIC.try_lock().expect("first thread to take lock") = Some(panic::take_hook());
    panic::set_hook(Box::new(panic_handler));

    // let buf = buffer::Buffer::new("./assets/test/passage_wrapped.txt").unwrap();
    // let buf = buffer::Buffer::new("./assets/test/crossbox.txt").unwrap();
    // let buf = buffer::Buffer::new_fromstring(String::new());
    // let buf = buffer::Buffer::new("./assets/test/lines.txt").unwrap();
    // let mut ctx = Ctx::from_buffer(libc::STDIN_FILENO, buf);

    let (send, recv) = tokio::sync::oneshot::channel::<()>();
    *EXIT_CHANNEL.lock().unwrap() = Some(send);
    let runtime = tokio::runtime::Builder::new_current_thread().enable_io().build().unwrap();
    let local_set = tokio::task::LocalSet::new();
    {
        let _lguard = local_set.enter();
        let _guard = runtime.enter();
        tokio::task::spawn_local(async {main_loop().await});
    }
    runtime.spawn_blocking(|| async move {recv.await.unwrap()});

    term::flush();
    term::altbuf_disable();
    println!();

    // eprintln!("reached end of main loop");
    if let Some(termios) = unsafe { &ORIGINAL_TERMIOS } {
        termios::tcsetattr(STDIN_FILENO, termios::SetArg::TCSANOW, termios).unwrap_or(());
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

    if let Some(termio) = unsafe { ORIGINAL_TERMIOS.clone() } {
        term::altbuf_disable();
        term::flush();
        termios::tcsetattr(STDIN_FILENO, termios::SetArg::TCSANOW, &termio).unwrap_or(());
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
