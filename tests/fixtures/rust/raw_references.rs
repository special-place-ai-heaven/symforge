#[repr(C)]
pub struct RegisterBlock {
    pub value: u32,
}

pub fn raw_reference_examples(register: &mut RegisterBlock) {
    let const_ptr = &raw const register.value;
    let mut_ptr = &raw mut register.value;

    unsafe {
        *mut_ptr = *const_ptr;
    }
}
