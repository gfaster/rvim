use guile_sys::*;

pub(super) unsafe trait ToScm: Send + Sync {
    unsafe fn to_scm(self) -> SCM;
}

const _: () = assert!(std::mem::size_of::<usize>() == std::mem::size_of::<u64>());
unsafe impl ToScm for usize {
    unsafe fn to_scm(self) -> SCM {
        scm_from_uint64(self as u64)
    }
}

unsafe impl ToScm for u64 {
    unsafe fn to_scm(self) -> SCM {
        scm_from_uint64(self)
    }
}

unsafe impl ToScm for u32 {
    unsafe fn to_scm(self) -> SCM {
        scm_from_uint32(self)
    }
}

unsafe impl ToScm for i32 {
    unsafe fn to_scm(self) -> SCM {
        scm_from_int32(self)
    }
}

unsafe impl ToScm for char {
    unsafe fn to_scm(self) -> SCM {
        // https://www.gnu.org/software/guile/manual/guile.html#index-scm_005finteger_005fto_005fchar
        let n = self as u32;
        let s = n.to_scm();
        scm_integer_to_char(s)
    }
}
