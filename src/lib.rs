#![doc = include_str!("../README.md")]
//! ## Available features
//! - `allocator-api`: Requires nightly and enables allocating with a specific [Allocator].
//!
//! [Allocator]: https://doc.rust-lang.org/std/alloc/trait.Allocator.html
#![no_std]
#![cfg_attr(feature = "allocator-api", feature(allocator_api))]

use core::{alloc::Layout, mem::MaybeUninit, ptr::NonNull};

use alloc_shim::{AllocError, Allocator, Global};

extern crate alloc;

mod alloc_shim;

pub struct Allocation<A: Allocator = Global> {
    // TODO: should be a Unique pointer!
    ptr: NonNull<u8>,
    layout: Layout,
    alloc: A,
}

fn match_allocated_size(ptr: NonNull<[u8]>, layout: Layout) -> (NonNull<u8>, Layout) {
    let actual_layout = unsafe { Layout::from_size_align_unchecked(ptr.len(), layout.align()) };
    debug_assert!(actual_layout.size() >= layout.size());
    (ptr.cast(), actual_layout)
}
fn allocate(alloc: &impl Allocator, layout: Layout) -> Result<(NonNull<u8>, Layout), AllocError> {
    let ptr = alloc.allocate(layout)?;
    Ok(match_allocated_size(ptr, layout))
}
unsafe fn grow(
    alloc: &impl Allocator,
    ptr: NonNull<u8>,
    old_layout: Layout,
    new_layout: Layout,
) -> Result<(NonNull<u8>, Layout), AllocError> {
    let ptr = alloc.grow(ptr, old_layout, new_layout)?;
    Ok(match_allocated_size(ptr, new_layout))
}
unsafe fn shrink(
    alloc: &impl Allocator,
    ptr: NonNull<u8>,
    old_layout: Layout,
    new_layout: Layout,
) -> Result<(NonNull<u8>, Layout), AllocError> {
    let ptr = alloc.shrink(ptr, old_layout, new_layout)?;
    Ok(match_allocated_size(ptr, new_layout))
}

/// Methods for the global allocator
impl Allocation {
    // Forwards to alloc, handles layout.size() == 0 with a dangling ptr
    pub fn new(layout: Layout) -> Self {
        Self::new_in(layout, Global)
    }
    pub fn into_parts(self) -> (NonNull<u8>, Layout) {
        let (ptr, layout, _) = Self::into_parts_with_alloc(self);
        (ptr, layout)
    }
    pub unsafe fn from_parts(ptr: NonNull<u8>, layout: Layout) -> Self {
        Self::from_parts_in(ptr, layout, Global)
    }
}
/// Common methods
impl<A: Allocator> Allocation<A> {
    pub fn as_slice(&self) -> NonNull<[MaybeUninit<u8>]> {
        let ptr = core::ptr::slice_from_raw_parts_mut(
            self.ptr.as_ptr().cast::<MaybeUninit<u8>>(),
            self.layout.size(),
        );
        unsafe { NonNull::new_unchecked(ptr) }
    }
    // Calls either grow or shrink, compares against stored layout
    pub fn realloc(&mut self, new_layout: Layout) {
        let () = self
            .try_realloc(new_layout)
            .unwrap_or_else(|AllocError| alloc::alloc::handle_alloc_error(new_layout));
    }
    pub fn layout(&self) -> Layout {
        self.layout
    }
}
/// Methods using the allocator-api or shim
impl<A: Allocator> Allocation<A> {
    pub fn new_in(layout: Layout, alloc: A) -> Self {
        Self::try_new_in(layout, alloc)
            .unwrap_or_else(|AllocError| alloc::alloc::handle_alloc_error(layout))
    }
    pub fn try_new_in(layout: Layout, alloc: A) -> Result<Self, AllocError> {
        let (ptr, layout) = allocate(&alloc, layout)?;
        Ok(Self { ptr, layout, alloc })
    }
    pub fn into_parts_with_alloc(self) -> (NonNull<u8>, Layout, A) {
        let me = core::mem::ManuallyDrop::new(self);
        let alloc = unsafe { core::ptr::read(&me.alloc) };
        (me.ptr, me.layout, alloc)
    }
    pub unsafe fn from_parts_in(ptr: NonNull<u8>, layout: Layout, alloc: A) -> Self {
        Self { ptr, layout, alloc }
    }
    pub fn try_realloc(&mut self, new_layout: Layout) -> Result<(), AllocError> {
        if new_layout == self.layout {
            return Ok(());
        }
        // Prefer grow to shrink when all we do is change alignment
        if new_layout.size() >= self.layout.size() {
            (self.ptr, self.layout) =
                unsafe { grow(&self.alloc, self.ptr, self.layout, new_layout)? };
            Ok(())
        } else {
            (self.ptr, self.layout) =
                unsafe { shrink(&self.alloc, self.ptr, self.layout, new_layout)? };
            Ok(())
        }
    }
}

impl<A: Allocator> Drop for Allocation<A> {
    fn drop(&mut self) {
        unsafe {
            self.alloc.deallocate(self.ptr, self.layout);
        }
    }
}

unsafe impl<A: Allocator + Sync> Sync for Allocation<A> {}
unsafe impl<A: Allocator + Send> Send for Allocation<A> {}

#[cfg(test)]
mod test;
