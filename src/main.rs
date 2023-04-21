#![allow(dead_code)]
mod render;
mod buffer;

use nix::sys::{signal::{self, SaFlags, SigHandler}, signalfd::SigSet};
use libc;
use render::Ctx;


fn main() {

    let sighandler = SigHandler::Handler(sa_handler);
    let sig = signal::SigAction::new(sighandler, SaFlags::empty(), SigSet::empty());

    unsafe {
        signal::sigaction(signal::Signal::SIGINT, &sig).unwrap();
    }

    let buf = buffer::Buffer::new("src/box2.txt").unwrap();
    let ctx = Ctx::new(0, buf);
    ctx.render();

    loop {
        
    }

    // eprintln!("reached end of main loop");
}

extern "C" fn sa_handler(_signum: libc::c_int) {
    // this is honestly terrifying. I love it
    // std::process::Command::new("/usr/bin/reset").spawn().unwrap();
    render::term::altbuf_disable();
    std::process::exit(0);
}
