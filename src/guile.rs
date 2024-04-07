//! I need to think about how I'm to actually implement this. I think the minimal implementation is
//! input a string in the command line, and execute a function to move the cursor.
//!
//! I need to be careful that Guile never unwinds into Rust and vice-versa. Continuations seem like
//! they'll be a major issue, and an easy way to create double-frees.
use std::{marker::PhantomData, process::abort};

use guile_sys::*;
use libc::c_void;

use crate::debug::log;

mod sealed {
    pub(super) struct Sealed;
}

fn rvim_init() {
    unsafe {
        scm_c_define_gsubr(c"pmsg".as_ptr(), 1, 0, 0, rscm_print_msg as *const unsafe extern "C" fn(SCM) -> SCM as *mut _);
        scm_c_define_gsubr(c"send-str".as_ptr(), 1, 0, 0, rscm_msg_chr as *const unsafe extern "C" fn(SCM) -> SCM as *mut _);
    }
}

/// Wrapper for scm objects so that they can be safely put on the Rust heap.
#[repr(transparent)]
pub struct ProtectedScm(SCM);

impl ProtectedScm {
    pub unsafe fn protect(obj: SCM) -> Self {
        unsafe { scm_gc_protect_object(obj) };
        ProtectedScm(obj)
    }
}

pub unsafe fn protect(obj: SCM) -> ProtectedScm {
    ProtectedScm::protect(obj)
}

impl Drop for ProtectedScm {
    fn drop(&mut self) {
        unsafe { scm_gc_unprotect_object(self.0) };
    }
}

/// I don't think I should ever really use this
#[allow(dead_code)]
#[clippy::has_significant_drop]
#[must_use = "if unused the context will immediately end"]
pub struct DynwindCtx {
    _sealed: sealed::Sealed,
    _not_send: PhantomData<*mut ()>,
}


impl DynwindCtx {
    /// create a new dynwind context. This is `unsafe` since leaking this is UB
    pub unsafe fn new() -> Self {
        scm_dynwind_begin(0);
        DynwindCtx { _sealed: sealed::Sealed, _not_send: PhantomData }
    }
}

impl Drop for DynwindCtx {
    fn drop(&mut self) {
        unsafe { scm_dynwind_end() };
    }
}

/// execute f with guile. `f` must never panic. I believe all types in `f` must have trivial drops,
/// since unwinding 
unsafe fn with_guile(f: impl FnOnce() -> *mut u8) -> *mut u8 {
    // TODO: optimize this implementation to not allocate
    // see https://internals.rust-lang.org/t/moving-out-of-raw-pointers-with-fnonce/16159
    // see also: panic::catch_unwind() implementation
    let mut wrapper: Option<Box<dyn FnOnce() -> *mut u8>> = Some(Box::new(f));
    unsafe extern "C" fn thunk(p: *mut c_void) -> *mut c_void {
        let Some(func) = ({
            (p as *mut Option<Box<dyn FnOnce() -> *mut u8>>).as_mut().and_then(|x| x.take())
        }) else {
            return std::ptr::null_mut();
        };
        func().cast()
    }
    scm_with_guile(Some(thunk), (&mut wrapper as *mut _) as *mut c_void) as *mut u8
}

/// for calling back into Rust from guile
fn rentry<T>(f: impl FnOnce() -> T + std::panic::UnwindSafe) -> T {
    let res = std::panic::catch_unwind(f).unwrap_or_else(|_| abort());
    res
}

pub fn execute_guile_interpreted(s: &str) -> Result<(), ()> {
    let s = s.trim_start();
    let ret = unsafe {
        with_guile(|| {
            let s_str = scm_from_utf8_stringn(s.as_ptr().cast(), s.len());
            let inport = scm_open_input_string(s_str);
            let read = scm_read(inport);
            let interaction_env = scm_interaction_environment();
            let ret = scm_eval(read, interaction_env);
            let port = scm_open_output_string();
            scm_display(ret, port);
            let s_out_str = scm_get_output_string(port);
            let mut len = 0;
            let msg = scm_to_utf8_stringn(s_out_str, &mut len);
            let msg = Box::new( Gmsg {
                len,
                msg,
            });
            Box::into_raw(msg) as *mut u8
        })
    };
    if ret.is_null() {
        Err(())
    } else {
        use crate::command::cmdline;
        let msg = unsafe { Box::from_raw(ret as *mut Gmsg) };
        let msg = cmdline::CmdMsg::Gmsg(*msg);
        cmdline::CommandLine::send_msg(msg)
    }
}

/// string from Guile that uses C malloc and free, must be valid utf8
pub struct Gmsg {
    len: usize,
    msg: *mut i8
}

unsafe impl Send for Gmsg {}
unsafe impl Sync for Gmsg {}

impl std::borrow::Borrow<str> for Gmsg {
    fn borrow(&self) -> &str {
        &*self
    }
}

impl std::ops::Deref for Gmsg {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        unsafe {
            let s = std::slice::from_raw_parts(self.msg as *const u8, self.len);
            std::str::from_utf8_unchecked(s)
        }
    }
}

impl std::fmt::Display for Gmsg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s: &str = self;
        <str as std::fmt::Display>::fmt(s, f)
    }
}

impl Drop for Gmsg {
    fn drop(&mut self) {
        unsafe { libc::free(self.msg.cast()) };
    }
}

fn to_scm_bool(b: bool) -> SCM {
    if b {
        unsafe { scm_bool_true() }
    } else {
        unsafe { scm_bool_false() }
    }
}

fn result_bool(r: Result<(), ()>) -> SCM {
    to_scm_bool(r.is_ok())
}

/// calls thunk which prints to current output port, and displays it in the command line.
pub unsafe extern "C" fn rscm_print_msg(thunk: SCM) -> SCM {
    use crate::command::cmdline;
    let s_out_str = scm_call_with_output_string(thunk);
    let mut len = 0;
    let msg = scm_to_utf8_stringn(s_out_str, &mut len);
    let msg = Gmsg {
        len,
        msg,
    };
    let msg = cmdline::CmdMsg::Gmsg(msg);
    result_bool(cmdline::CommandLine::send_msg(msg))
}

pub unsafe extern "C" fn rscm_msg_chr(ch: SCM) -> SCM {
    use crate::command::cmdline;
    let mut len = 0;
    let msg = scm_to_utf8_stringn(ch, &mut len);
    let msg = Gmsg {
        len,
        msg,
    };
    let msg = cmdline::CmdMsg::Gmsg(msg);
    result_bool(cmdline::CommandLine::send_msg(msg))
}




pub fn initialize() {
    let ret = unsafe {
        with_guile(|| {
            rvim_init();
            scm_c_primitive_load(c"base.scm".as_ptr());
            std::ptr::NonNull::dangling().as_ptr()
        })
    };
    if ret.is_null() {
        log!("failed to initialize scheme")
    }
}
