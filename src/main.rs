#![allow(dead_code)]
mod buffer;
mod input;
mod render;
mod term;
mod textobj;
mod window;
use nix::sys::{
    signal::{self, SaFlags, SigHandler},
    signalfd::SigSet,
};
use render::Ctx;
use std::sync::atomic::AtomicBool;

pub enum Mode {
    Normal,
    Insert,
}

// how I handle the interrupts for now
static mut PENDING: AtomicBool = AtomicBool::new(false);

fn main() {
    let sighandler = SigHandler::Handler(sa_handler);
    let sig = signal::SigAction::new(sighandler, SaFlags::empty(), SigSet::empty());

    unsafe {
        signal::sigaction(signal::Signal::SIGINT, &sig).unwrap();
    }

    // let buf = buffer::Buffer::new("src/passage_wrapped.txt").unwrap();
    let buf = buffer::Buffer::new("src/lines.txt").unwrap();
    let mut ctx = Ctx::new(libc::STDIN_FILENO, buf);
    ctx.render();

    loop {
        // Todo: run on separate thread
        if let Some(token) = input::handle_input(&ctx) {
            ctx.process_token(token);
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
}

extern "C" fn sa_handler(_signum: libc::c_int) {
    // this is honestly terrifying. I love it
    // std::process::Command::new("/usr/bin/reset").spawn().unwrap();
    unsafe {
        PENDING = true.into();
    }
}
