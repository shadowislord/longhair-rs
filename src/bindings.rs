use std::os::raw::{c_int, c_uchar, c_uint};

pub const CAUCHY_256_VERSION: c_uint = 2;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct Block {
    pub data: *mut c_uchar,
    pub row: c_uchar,
}

extern "C" {
    pub fn _cauchy_256_init(expected_version: c_int) -> c_int;
    pub fn cauchy_256_encode(
        k: c_int,
        m: c_int,
        data_ptrs: *mut *const c_uchar,
        recovery_blocks: *mut *mut c_uchar,
        block_bytes: c_int,
    ) -> c_int;
    pub fn cauchy_256_decode(k: c_int, m: c_int, blocks: *mut Block, block_bytes: c_int) -> c_int;
}
