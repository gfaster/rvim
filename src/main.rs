#![allow(dead_code)]
mod render;
mod buffer;

use std::sync::atomic::AtomicBool;

use nix::{sys::{signal::{self, SaFlags, SigHandler}, signalfd::SigSet}, unistd::pause};
use libc;
use render::Ctx;

// how I handle the interrupts for now
static mut PENDING: AtomicBool = AtomicBool::new(false);

fn main() {

    let sighandler = SigHandler::Handler(sa_handler);
    let sig = signal::SigAction::new(sighandler, SaFlags::empty(), SigSet::empty());

    unsafe {
        signal::sigaction(signal::Signal::SIGINT, &sig).unwrap();
    }

    let buf = buffer::Buffer::new("src/box2.txt").unwrap();
    let ctx = Ctx::new(libc::STDIN_FILENO, buf);
    ctx.render();

    loop {
        pause();
        unsafe {
            if PENDING.load(std::sync::atomic::Ordering::Acquire) {
                break;
            }
        }
    }

    render::term::altbuf_disable();
    render::term::flush();

    // eprintln!("reached end of main loop");
}

extern "C" fn sa_handler(_signum: libc::c_int) {
    // this is honestly terrifying. I love it
    // std::process::Command::new("/usr/bin/reset").spawn().unwrap();
    unsafe {
        PENDING = true.into();
    }
}
