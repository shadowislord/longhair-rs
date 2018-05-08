#![allow(non_snake_case)]

use std::borrow::{Borrow, BorrowMut};
use std::sync::{Once, ONCE_INIT};

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

static CAUCHY_INITIALIZED: Once = ONCE_INIT;

pub trait AsBlock {
    fn data_mut(&mut self) -> &mut [u8];
    fn data(&self) -> &[u8];
    fn index(&self) -> u32;
    fn set_index(&mut self, index: u32);
}

pub struct Cauchy {
    max_k: u32,
    block_ptrs: Vec<*const u8>,
    recovery_block_ptrs: Vec<*mut u8>,
    native_block_ptrs: Vec<_Block>,
}

impl Cauchy {
    pub fn new(max_k: u32) -> Cauchy {
        if max_k > 256 {
            panic!("k must be <= 256");
        }

        CAUCHY_INITIALIZED.call_once(|| unsafe {
            if _cauchy_256_init(CAUCHY_256_VERSION as i32) != 0 {
                panic!("cauchy initialize failed!");
            }
        });

        Cauchy {
            max_k,
            block_ptrs: Vec::with_capacity(max_k as usize),
            recovery_block_ptrs: Vec::with_capacity(max_k as usize),
            native_block_ptrs: Vec::with_capacity(max_k as usize),
        }
    }

    pub fn max_k(&self) -> u32 {
        self.max_k
    }

    pub fn encode<I: Borrow<[u8]>, O: BorrowMut<[u8]>>(
        &mut self,
        blocks: &[I],
        recovery_blocks: &mut [O],
    ) {
        if blocks.len() as u32 > self.max_k {
            panic!("num blocks must be <= max_k");
        }

        // println!("longhair-rs: recovery_blocks {:?}", recovery_blocks);
        // println!(
        //     "longhair-rs: recovery_blocks.len(): {}",
        //     recovery_blocks.len()
        // );

        self.block_ptrs.clear();

        let mut block_bytes_opt = None;
        for i in 0..blocks.len() {
            let block = blocks[i].borrow();
            if let Some(block_bytes) = block_bytes_opt {
                if block.len() != block_bytes {
                    panic!("all blocks must have the same size");
                }
            } else {
                block_bytes_opt = Some(block.len());
            }

            self.block_ptrs.push(block.as_ptr() as *const u8);
        }

        let block_bytes = block_bytes_opt.unwrap();
        if block_bytes == 0 || block_bytes % 8 != 0 {
            panic!("the size of data blocks cannot be zero and must be a multiple of 8");
        }

        self.recovery_block_ptrs.clear();
        for i in 0..recovery_blocks.len() {
            let recovery_block = recovery_blocks[i].borrow_mut();
            if block_bytes != recovery_block.len() {
                panic!("all blocks must have the same size");
            }
            self.recovery_block_ptrs
                .push(recovery_block.as_mut_ptr() as *mut u8);
        }

        // println!("longhair-rs: block_ptrs: {:?}", self.block_ptrs);
        // println!(
        //     "longhair-rs: recovery_block_ptrs: {:?}",
        //     self.recovery_block_ptrs
        // );

        let block_ptrs = self.block_ptrs.as_ptr() as *mut *const u8;
        let buf_ptr = self.recovery_block_ptrs.as_mut_ptr() as *mut *mut u8;

        let result = unsafe {
            cauchy_256_encode(
                blocks.len() as i32,
                self.recovery_block_ptrs.len() as i32,
                block_ptrs,
                buf_ptr,
                block_bytes as i32,
            )
        };

        if result != 0 {
            panic!("cauchy encode failed!");
        }
    }

    pub fn decode<B: AsBlock>(&mut self, k: u32, m: u32, blocks: &mut [B]) {
        if k > self.max_k {
            panic!("k must be <= max_k");
        }
        if blocks.len() != k as usize {
            panic!("blocks len must be the same as k");
        }

        self.native_block_ptrs.clear();

        let mut block_bytes_opt = None;
        for block in blocks.iter_mut() {
            let index = block.index();
            let data = block.data_mut();
            if let Some(block_bytes) = block_bytes_opt {
                if data.len() != block_bytes {
                    panic!("all blocks must have the same size");
                }
            } else {
                block_bytes_opt = Some(data.len());
            }

            if index >= k + m {
                panic!("block number cannot be >= k + m");
            }

            self.native_block_ptrs.push(_Block {
                row: index as u8,
                data: data.as_ptr() as *mut u8,
            });
        }

        let block_bytes = block_bytes_opt.unwrap();
        if block_bytes == 0 || block_bytes % 8 != 0 {
            panic!("the size of blocks cannot be zero and must be a multiple of 8");
        }

        let block_ptrs = self.native_block_ptrs.as_mut_ptr();

        let result =
            unsafe { cauchy_256_decode(k as i32, m as i32, block_ptrs, block_bytes as i32) };

        if result != 0 {
            panic!("cauchy decode failed!");
        }

        for (input_block, native_block) in blocks.iter_mut().zip(self.native_block_ptrs.iter()) {
            input_block.set_index(native_block.row as u32);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl<'a> AsBlock for (u32, &'a mut [u8]) {
        fn set_index(&mut self, index: u32) {
            self.0 = index;
        }
        fn index(&self) -> u32 {
            self.0
        }
        fn data_mut(&mut self) -> &mut [u8] {
            self.1
        }
        fn data(&self) -> &[u8] {
            self.1
        }
    }

    #[test]
    fn encode_and_decode() {
        let mut cauchy = Cauchy::new(2);
        let mut d = vec![[0, 1, 2, 3, 4, 5, 6, 7], [8, 9, 10, 11, 12, 13, 14, 15]];
        let mut r = vec![[0; 8], [0; 8]];

        cauchy.encode(&d[..], &mut r[..]);
        assert_eq!([8, 8, 8, 8, 8, 8, 8, 8], r[0]);
        assert_eq!([0, 9, 11, 9, 15, 9, 11, 9], r[1]);

        let mut recover_me: [(u32, &mut [u8]); 2] =
            [(3, &mut r[1].clone()), (1, &mut d[1].clone())];
        cauchy.decode(2, 2, &mut recover_me);
        assert_data_recovered_ok(&recover_me);

        let mut recover_me: [(u32, &mut [u8]); 2] =
            [(0, &mut d[0].clone()), (3, &mut r[1].clone())];
        cauchy.decode(2, 2, &mut recover_me);
        assert_data_recovered_ok(&recover_me);

        let mut recover_me: [(u32, &mut [u8]); 2] =
            [(2, &mut r[0].clone()), (3, &mut r[1].clone())];
        cauchy.decode(2, 2, &mut recover_me);
        assert_data_recovered_ok(&recover_me);
    }

    fn assert_data_recovered_ok<B: AsBlock>(recover_me: &[B]) {
        assert_eq!(0, recover_me[0].index());
        assert_eq!(1, recover_me[1].index());
        assert_eq!(&[0, 1, 2, 3, 4, 5, 6, 7], recover_me[0].data());
        assert_eq!(&[8, 9, 10, 11, 12, 13, 14, 15], recover_me[1].data());
    }
}
