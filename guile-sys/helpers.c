#include<libguile.h>


SCM 
guile_sys_bool_true(void)
{
	return SCM_BOOL_T;
}

SCM 
guile_sys_bool_false(void)
{
	return SCM_BOOL_F;
}

int 
guile_sys_sizeof_scm(void)
{
	return sizeof(SCM);
}


SCM guile_sys_unspecified(void) {
	return SCM_UNSPECIFIED;
}
