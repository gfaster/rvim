#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[repr(transparent)]
pub struct SCM(*mut ());

unsafe impl Send for SCM {}
unsafe impl Sync for SCM {}

pub const SCM_BOOL_T: SCM = SCM(0x404 as _);
pub const SCM_BOOL_F: SCM = SCM(0x4 as _);
pub const SCM_UNSPECIFIED: SCM = SCM(0x804 as _);
