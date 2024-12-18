/// Run with
///
/// ```bash
/// cargo test
/// cargo +nightly miri test
/// cargo +nightly miri test --features allocator-api
/// ```
use alloc::{boxed::Box, vec::Vec};

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
    let (ptr, _) = alloc.into_parts();
    let _ = unsafe { Box::<i32>::from_raw(ptr.as_ptr().cast()) };
}

#[test]
fn convert_vec() {
    let empty_alloc = Allocation::new(Layout::new::<[i32; 0]>());
    let (ptr, _) = empty_alloc.into_parts();
    let _ = unsafe { Vec::<i32>::from_raw_parts(ptr.as_ptr().cast(), 0, 0) };

    let filled_alloc = Allocation::new(Layout::new::<[i32; 32]>());
    let (ptr, _) = filled_alloc.into_parts();
    let _ = unsafe { Vec::<i32>::from_raw_parts(ptr.as_ptr().cast(), 0, 32) };
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
