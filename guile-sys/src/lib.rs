#![allow(
    non_camel_case_types,
    non_upper_case_globals,
    non_snake_case,
    improper_ctypes
)]

mod bindings;
pub use bindings::*;

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn scm_type_is_expected_size() {
        let real_sz = unsafe { guile_sys_sizeof_scm() };
        assert_eq!(std::mem::size_of::<SCM>(), real_sz as usize);
    }

    #[test]
    fn bool_consts_correct() {
        let t = unsafe { guile_sys_bool_true() };
        let f = unsafe { guile_sys_bool_false() };
        assert_eq!(t, SCM_BOOL_T);
        assert_eq!(f, SCM_BOOL_F);
    }

    #[test]
    fn consts_correct() {
        let ty = unsafe { guile_sys_unspecified() };
        assert_eq!(ty, SCM_UNSPECIFIED);
    }
}
