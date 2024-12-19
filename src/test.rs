//! Run with
//!
//! ```bash
//! cargo test
//! cargo +nightly miri test
//! cargo +nightly miri test --features allocator-api
//! ```

use alloc::boxed::Box;
use core::alloc::Layout;

use crate::*;

#[test]
fn test_alloc() {
    let _ = Allocation::new(Layout::from_size_align(0, 1).unwrap());
    let _ = Allocation::new(Layout::from_size_align(1, 1).unwrap());
    let _ = Allocation::new(Layout::from_size_align(4, 4).unwrap());
    let _ = Allocation::new(Layout::from_size_align(1_048_576, 32).unwrap());
    let _ = Allocation::new(Layout::from_size_align(1_048_576, 65536).unwrap());
}

#[test]
fn test_realloc() {
    let mut alloc = Allocation::new(Layout::from_size_align(4, 4).unwrap());
    alloc.realloc(Layout::from_size_align(32, 4).unwrap());
    alloc.realloc(Layout::from_size_align(32, 65536).unwrap());
}

#[test]
fn test_data() {
    let alloc = Allocation::new(Layout::new::<i32>());
    // This test is run under miri, so ensures that the pointer is valid for reads and writes
    let ptr = alloc.as_slice().as_ptr() as *mut u8 as *mut u32;
    *unsafe { &mut *ptr } = 0xdead;
    assert_eq!(unsafe { core::ptr::read(ptr) }, 0xdead);
    *unsafe { &mut *ptr } = 1000;
    assert_eq!(unsafe { core::ptr::read(ptr) }, 1000);
}

#[test]
fn convert_box() {
    let alloc = Allocation::new(Layout::new::<i32>());
    let _boxed = alloc.try_into_box::<i32>().unwrap();

    let boxed = Box::new(42);
    let _alloc = Allocation::from(boxed);
}

#[test]
fn convert_vec() {
    let empty_alloc = Allocation::new(Layout::new::<[i32; 0]>());
    let vec = empty_alloc.try_into_vec::<i32>().unwrap();
    assert_eq!(vec.capacity(), 0);

    let filled_alloc = Allocation::new(Layout::new::<[i32; 32]>());
    let vec = filled_alloc.try_into_vec::<i32>().unwrap();
    assert_eq!(vec.capacity(), 32);

    // TODO: implement a cast for ZST with size hints?
}

#[test]
fn zeroed() {
    let alloc = Allocation::zeroed(Layout::new::<i32>());
    assert_eq!(
        *unsafe { alloc.as_uninit_ref::<i32>().assume_init_ref() },
        0
    );

    let mut alloc = Allocation::new(Layout::new::<[i32; 1]>());
    unsafe {
        alloc.as_ptr::<i32>().write(42);
    }
    alloc.realloc_zeroed(Layout::new::<[i32; 2]>());
    assert_eq!(
        unsafe { alloc.as_uninit_ref::<[i32; 2]>().assume_init_ref() },
        &[42, 0]
    );
    alloc.realloc_zeroed(Layout::new::<[i32; 1]>());
    assert_eq!(
        *unsafe { alloc.as_uninit_ref::<i32>().assume_init_ref() },
        42
    );
    alloc.realloc_zeroed(Layout::new::<[i32; 2]>());
    assert_eq!(
        unsafe { alloc.as_uninit_ref::<[i32; 2]>().assume_init_ref() },
        &[42, 0]
    );
}
