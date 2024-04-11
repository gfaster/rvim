//! I need to think about how I'm to actually implement this. I think the minimal implementation is
//! input a string in the command line, and execute a function to move the cursor.
//!
//! I need to be careful that Guile never unwinds into Rust and vice-versa. Continuations seem like
//! they'll be a major issue, and an easy way to create double-frees.
use std::{marker::PhantomData, mem::{ManuallyDrop, MaybeUninit}, process::abort, ptr, sync::Arc};

use guile_sys::*;
use libc::c_void;

mod utils;
mod convert;
use convert::ToScm;

use crate::{buffer::Buffer, debug::log};

mod sealed {
    pub(super) struct Sealed;
}

type ScmFn0 = unsafe extern "C" fn() -> SCM;
type ScmFn1 = unsafe extern "C" fn(SCM) -> SCM;
type ScmFn2 = unsafe extern "C" fn(SCM, SCM) -> SCM;
type ScmFn3 = unsafe extern "C" fn(SCM, SCM, SCM) -> SCM;

fn rvim_init() {
    unsafe {
        ScmBufferRef::rscm_init();

        let f: ScmFn1 = rscm_msg_chr;
        scm_c_define_gsubr(c"rs-send-str".as_ptr(), 1, 0, 0, f as *mut _);

        let f: ScmFn0 = rscm_current_buffer;
        scm_c_define_gsubr(c"rs-curr-buf".as_ptr(), 0, 0, 0, f as *mut _);

        let f: ScmFn2 = rscm_char_after;
        scm_c_define_gsubr(c"rs-char-after".as_ptr(), 2, 0, 0, f as *mut _);

        let f: ScmFn1 = rscm_curr_pos;
        scm_c_define_gsubr(c"rs-curr-pos".as_ptr(), 1, 0, 0, f as *mut _);

        let f: ScmFn3 = rscm_insert_str;
        scm_c_define_gsubr(c"rs-insert-str".as_ptr(), 3, 0, 0, f as *mut _);
    }
}

/// Wrapper for scm objects so that they can be safely put on the Rust heap.
#[repr(transparent)]
#[derive(Debug, PartialEq, Eq)]
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

// /// Makes an `Arc<T>` into a scheme pointer object. The refcount will be decremented on GC.
// ///
// /// The user must take care to not mutate the returned value
// pub fn scm_arc<T>(arc: std::sync::Arc<T>) -> ProtectedScm {
//     let p = std::sync::Arc::into_raw(arc);
//     let ret = unsafe {
//         with_guile(|| {
//
//         })
//     }
// }

/*
/// execute f with guile. `f` must never panic. I believe all types in `f` must have trivial drops,
/// since unwinding 
unsafe fn with_guile_general(f: impl FnOnce() -> *mut u8) -> *mut u8 {
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
*/

/// execute f with guile. `f` must never panic. I believe all types in `f` must have trivial drops,
/// since unwinding.
unsafe fn with_guile<F, T>(f: F) -> Option<T>
where
    F: FnOnce() -> T
{
    union Func<F, T> {
        f: ManuallyDrop<F>,
        t: ManuallyDrop<T>,
    }

    let mut func = Func {
        f: ManuallyDrop::new(f),
    };
    
    unsafe extern "C" fn thunk<F, T>(p: *mut c_void) -> *mut c_void 
    where
        F: FnOnce() -> T
    {
        let p: *mut Func<F, T> = p.cast();
        let f = ManuallyDrop::into_inner(ptr::read(&(*p).f));
        let res = f();
        (*p).t = ManuallyDrop::new(res);

        p.cast()
    }

    let f: *mut Func<_, _> = &mut func;
    let res = scm_with_guile(Some(thunk::<F, T>), f.cast());

    if res.is_null() {
        None
    } else {
        Some(ManuallyDrop::into_inner(func.t))
    }
}

/// for calling back into Rust from guile. I think I might make this throw in the future
unsafe fn reentry<T>(f: impl FnOnce() -> T + std::panic::UnwindSafe) -> T {
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
            Gmsg {
                len,
                msg,
            }
        })
    };
    use crate::command::cmdline;
    let msg = ret.ok_or(())?;
    let msg = cmdline::CmdMsg::Gmsg(msg);
    cmdline::CommandLine::send_msg(msg)
}

/// string from Guile that uses C malloc and free, must be valid utf8
pub struct Gmsg {
    len: usize,
    msg: *mut i8
}

unsafe impl Send for Gmsg {}
unsafe impl Sync for Gmsg {}

impl Gmsg {
    unsafe fn from_scm(obj: SCM) -> Self {
        let mut len = 0;
        let msg = scm_to_utf8_stringn(obj, &mut len);
        Gmsg {
            len,
            msg,
        }
    }
}

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
        SCM_BOOL_T
    } else {
        SCM_BOOL_F
    }
}

fn result_bool(r: Result<(), ()>) -> SCM {
    to_scm_bool(r.is_ok())
}

unsafe fn rscm_unwrap_soft<T: ToScm>(opt: Option<T>) -> SCM {
    match opt {
        Some(opt) => opt.to_scm(),
        None => SCM_BOOL_F,
    }
}

/// calls thunk which prints to current output port, and displays it in the command line.
pub unsafe extern "C" fn rscm_print_msg(thunk: SCM) -> SCM {
    use crate::command::cmdline;
    let s_out_str = scm_call_with_output_string(thunk);
    let msg = Gmsg::from_scm(s_out_str);
    let msg = cmdline::CmdMsg::Gmsg(msg);
    result_bool(cmdline::CommandLine::send_msg(msg))
}

pub unsafe extern "C" fn rscm_msg_chr(ch: SCM) -> SCM {
    use crate::command::cmdline;
    let msg = Gmsg::from_scm(ch);
    let msg = cmdline::CmdMsg::Gmsg(msg);
    result_bool(cmdline::CommandLine::send_msg(msg))
}

unsafe fn rscm_from_str_symbol(s: &str) -> SCM {
    scm_from_utf8_symboln(s.as_ptr().cast(), s.len())
}

unsafe trait ScmRef {
    unsafe fn ty() -> SCM;
}


/// convert a scheme object to a pointer, throws a scheme exception if wrong type
unsafe fn rscm_as_ty<T: ScmRef>(obj: SCM) -> *const T {
    let ty = T::ty();
    if BUF_REF_TY == SCM_UNSPECIFIED {
        abort();
    };
    scm_assert_foreign_object_type(ty, obj);

    let s: *const T = scm_foreign_object_ref(obj, 0).cast();

    if s.is_null() {
        abort()
    }

    s
}

pub struct ScmBufferRef;
static mut BUF_REF_TY: SCM = SCM_UNSPECIFIED;

impl ScmBufferRef {
    unsafe extern "C" fn rscm_finalizer(obj: SCM) {
        let s: *const Buffer = rscm_as_ty(obj);
        let _ = std::sync::Arc::from_raw(s);
        // since this is a finalizer, I don't think I need to remember it
    }

    unsafe fn rscm_init() {
        let name = rscm_from_str_symbol("buffer");
        let slots = scm_list_1(rscm_from_str_symbol("data"));
        let ty = scm_make_foreign_object_type(name, slots, Some(Self::rscm_finalizer));
        BUF_REF_TY = ty;
    }
}

unsafe impl ScmRef for Buffer {
    unsafe fn ty() -> SCM {
        BUF_REF_TY
    }
}

pub unsafe extern "C" fn rscm_current_buffer() -> SCM {
    let curr = reentry(|| crate::render::CURRENT_BUF.get());
    let Some(curr) = curr else {
        return SCM_BOOL_F
    };

    let raw: *const Buffer = Arc::into_raw(curr);
    scm_make_foreign_object_1(BUF_REF_TY, raw as *mut _)
}

pub unsafe extern "C" fn rscm_char_after(buf: SCM, pos: SCM) -> SCM {
    let p: *const Buffer = rscm_as_ty(buf);
    let pos = scm_to_uint64(pos) as usize;
    let ch = reentry(|| {
        let guard = (*p).get();
        if pos < guard.len() {
            Some(guard.char_at(pos))
        } else {
            None
        }
    });
    rscm_unwrap_soft(ch)
}

pub unsafe extern "C" fn rscm_curr_pos(buf: SCM) -> SCM {
    let p: *const Buffer = rscm_as_ty(buf);
    let pos = reentry(|| {
        let guard = (*p).get();
        guard.pos_to_offset(guard.cursor.pos)
    });
    scm_from_uint64(pos as u64)
}

pub unsafe extern "C" fn rscm_insert_str(buf: SCM, pos: SCM, string: SCM) -> SCM {
    let p: *const Buffer = rscm_as_ty(buf);
    let pos = scm_to_uint64(pos) as usize;
    let s = Gmsg::from_scm(string);
    reentry(|| {
        let mut guard = (*p).get_mut();
        if pos < guard.len() {
            guard.cursor.pos = guard.offset_to_pos(pos);
            guard.insert_str(&s);
        } else {
            ()
        }
    });
    SCM_UNSPECIFIED
}

pub fn initialize() {
    static ONCE: std::sync::Once = std::sync::Once::new();

    ONCE.call_once(||unsafe {
        let ret = with_guile(|| {
            rvim_init();
            scm_c_primitive_load(c"base.scm".as_ptr());
        });
        if ret.is_none() {
            log!("failed to initialize scheme")
        }
    })
}
