#![allow(non_snake_case)]

#[macro_use]
#[cfg(test)]
extern crate proptest;

#[cfg(test)]
extern crate rand;

use std::borrow::{Borrow, BorrowMut};
use std::sync::{Once, ONCE_INIT};

include!("bindings.rs");

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
    native_block_ptrs: Vec<Block>,
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
        for block in blocks {
            let data = block.borrow();
            if let Some(block_bytes) = block_bytes_opt {
                if data.len() != block_bytes {
                    panic!("all blocks must have the same size");
                }
            } else {
                block_bytes_opt = Some(data.len());
            }

            self.block_ptrs.push(data.as_ptr() as *const u8);
        }

        let block_bytes = block_bytes_opt.unwrap();
        if block_bytes == 0 || block_bytes % 8 != 0 {
            panic!("the size of data blocks cannot be zero and must be a multiple of 8");
        }

        self.recovery_block_ptrs.clear();
        for recovery_block in recovery_blocks {
            let data = recovery_block.borrow_mut();
            if block_bytes != data.len() {
                panic!("all blocks must have the same size");
            }
            self.recovery_block_ptrs.push(data.as_mut_ptr() as *mut u8);
        }

        // println!("longhair-rs: block_bytes: {}", block_bytes);
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

            self.native_block_ptrs.push(Block {
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
    use proptest::prelude::*;
    use rand::{thread_rng, Rng};

    impl<'a, T> AsBlock for (u32, &'a mut T)
    where
        T: Borrow<[u8]>,
        T: BorrowMut<[u8]>,
    {
        fn set_index(&mut self, index: u32) {
            self.0 = index;
        }
        fn index(&self) -> u32 {
            self.0
        }
        fn data_mut(&mut self) -> &mut [u8] {
            &mut *self.1.borrow_mut()
        }
        fn data(&self) -> &[u8] {
            &*(*self.1).borrow()
        }
    }

    type Block = Box<[u8]>;

    fn generate_block(block_size: usize) -> Block {
        let mut b = vec![0_u8; block_size];
        thread_rng().fill(&mut *b);
        b.into_boxed_slice()
    }

    fn empty_blocks(block_size: usize, num_blocks: u32) -> Vec<Block> {
        let mut blocks = Vec::with_capacity(num_blocks as usize);
        for _ in 0..num_blocks {
            blocks.push(vec![0_u8; block_size].into_boxed_slice());
        }
        blocks
    }

    prop_compose! {
        fn block_size(max: usize)(base in 1..=max/8) -> usize {
            base * 8
        }
    }

    prop_compose! {
        fn random_block(block_size: usize)(size in Just(block_size))
            -> Box<[u8]> {
            generate_block(size)
        }
    }

    prop_compose! {
        fn random_blocks(max_size: usize)
                 (block_size in block_size(max_size))
                 (v in prop::collection::vec(random_block(block_size), 1..32)) -> Vec<Block> {
            v
        }
    }

    proptest! {
        #[test]
        fn encode_decode(input in random_blocks(512), output_blocks in 2u32..32) {
            let input_blocks = input.len() as u32;
            let block_size = input[0].len();
            prop_assume!(input_blocks + output_blocks <= 255);

            let mut output = empty_blocks(block_size, output_blocks);
            let mut cauchy = Cauchy::new(input_blocks as u32);

            cauchy.encode(&input, &mut output);

            let mut input_cloned = input.clone();
            let mut output_with_indices = input_cloned.iter_mut().chain(output.iter_mut())
                                        .enumerate()
                                        .map(|(i,b)| (i as u32, b))
                                        .collect::<Vec<_>>();
            thread_rng().shuffle(&mut output_with_indices);
            output_with_indices.truncate(input_blocks as usize);
            cauchy.decode(input_blocks, output_blocks, &mut output_with_indices[..]);
            output_with_indices.sort_by_key(|&(i,_)|i);

            for (expected_index, expected_block) in input.iter().enumerate() {
                let &(actual_index, ref actual_block) = &output_with_indices[expected_index];
                assert_eq!(expected_index as u32, actual_index);
                assert_eq!(expected_block, *actual_block);
            }
        }
    }
}
