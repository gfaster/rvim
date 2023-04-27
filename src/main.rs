#![allow(dead_code)]
mod buffer;
mod input;
mod render;
mod term;
mod textobj;
mod window;
use nix::sys::termios::{self, Termios};
use buffer::PTBuffer;
use libc::STDIN_FILENO;
use nix::sys::{
    signal::{self, SaFlags, SigHandler},
    signalfd::SigSet,
};
use render::Ctx;
use std::{path::Path, sync::atomic::AtomicBool, panic::{PanicInfo, self}};

#[derive(Clone, Copy)]
pub enum Mode {
    Normal,
    Insert,
}

// how I handle the interrupts for now
static mut PENDING: AtomicBool = AtomicBool::new(false);
static mut ORIGINAL_TERMIOS: Option<Termios> = None;

fn main() {

    // setup interrupt handling 
    let sighandler = SigHandler::Handler(sa_handler);
    let sig = signal::SigAction::new(sighandler, SaFlags::empty(), SigSet::empty());
    unsafe {
        signal::sigaction(signal::Signal::SIGINT, &sig).unwrap();
    }

    // panic handler is needed because we need to restore the terminal
    unsafe {
        ORIGINAL_TERMIOS = Some(termios::tcgetattr(STDIN_FILENO).unwrap());
    }
    panic::set_hook(Box::new(panic_handler));

    // let buf = buffer::Buffer::new("./assets/test/passage_wrapped.txt").unwrap();
    // let buf = buffer::Buffer::new("./assets/test/crossbox.txt").unwrap();
    // let buf = buffer::Buffer::new_fromstring(String::new());
    // let buf = buffer::Buffer::new("./assets/test/lines.txt").unwrap();
    // let mut ctx = Ctx::from_buffer(libc::STDIN_FILENO, buf);
    let mut ctx: Ctx<PTBuffer> =
        Ctx::from_file(libc::STDIN_FILENO, Path::new("./assets/test/passage_wrapped.txt")).unwrap();
    ctx.render();

    loop {
        // Todo: run on separate thread
        if let Some(token) = input::handle_input(&ctx) {
            ctx.process_action(token);
            ctx.render();
        }

        unsafe {
            if PENDING.load(std::sync::atomic::Ordering::Acquire) {
                break;
            }
        }
    }

    term::altbuf_disable();
    term::flush();

    // eprintln!("reached end of main loop");
    unsafe {
        if let Some(termios) = &ORIGINAL_TERMIOS {
            termios::tcsetattr(STDIN_FILENO, termios::SetArg::TCSANOW, &termios).unwrap_or(());
        } else {
            panic!("unable to reset terminal");
        }
    }
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

    eprintln!("DON'T PANIC, it said in large, friendly letters.");

    if let Some(location) = pi.location() {
        eprint!("Panicked at {}: {} ", location.file(), location.line());
    }

    if let Some(payload) = pi.payload().downcast_ref::<String>() {
        eprint!("on: {:?}", payload);
    }
    eprint!("\n\n");
}

extern "C" fn sa_handler(_signum: libc::c_int) {
    // this is honestly terrifying. I love it
    // std::process::Command::new("/usr/bin/reset").spawn().unwrap();
    unsafe {
        PENDING = true.into();
    }
}
