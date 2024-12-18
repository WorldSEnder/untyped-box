#[cfg(feature = "allocator-api")]
pub use alloc::alloc::{AllocError, Allocator, Global};

#[cfg(not(feature = "allocator-api"))]
mod shim {
    use core::{alloc::Layout, ptr::NonNull};
    fn write_zeroes(ptr: *mut [u8]) {
        unsafe { core::ptr::write_bytes(ptr as *mut u8, 0, ptr.len()) };
    }

    pub struct AllocError;
    pub trait Allocator {
        fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError>;
        fn allocate_zeroed(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError>;
        unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout);
        unsafe fn grow(
            &self,
            ptr: NonNull<u8>,
            old_layout: Layout,
            new_layout: Layout,
        ) -> Result<NonNull<[u8]>, AllocError>;
        unsafe fn grow_zeroed(
            &self,
            ptr: NonNull<u8>,
            old_layout: Layout,
            new_layout: Layout,
        ) -> Result<NonNull<[u8]>, AllocError>;
        unsafe fn shrink(
            &self,
            ptr: NonNull<u8>,
            old_layout: Layout,
            new_layout: Layout,
        ) -> Result<NonNull<[u8]>, AllocError>;
    }
    pub struct Global;
    impl Allocator for Global {
        fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
            if layout.size() == 0 {
                let ptr = core::ptr::null_mut::<u8>().wrapping_add(layout.align());
                let slice = core::ptr::slice_from_raw_parts_mut(ptr, 0);
                Ok(unsafe { NonNull::new_unchecked(slice) })
            } else {
                let ptr = unsafe { alloc::alloc::alloc(layout) };
                let slice = core::ptr::slice_from_raw_parts_mut(ptr, layout.size());
                Ok(NonNull::new(slice).ok_or(AllocError)?)
            }
        }
        fn allocate_zeroed(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
            let alloc = self.allocate(layout)?;
            // memory is fresh so in particular not aliased
            write_zeroes(alloc.as_ptr());
            Ok(alloc)
        }

        unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
            if layout.size() == 0 {
                // do nothing
            } else {
                alloc::alloc::dealloc(ptr.as_ptr(), layout);
            }
        }

        unsafe fn grow(
            &self,
            old_ptr: NonNull<u8>,
            old_layout: Layout,
            new_layout: Layout,
        ) -> Result<NonNull<[u8]>, AllocError> {
            if old_layout.align() == new_layout.align() {
                let new_size = new_layout.size();
                unsafe { core::hint::assert_unchecked(new_size >= old_layout.size()) };
                let ptr = alloc::alloc::realloc(old_ptr.as_ptr(), old_layout, new_size);
                let slice = core::ptr::slice_from_raw_parts_mut(ptr, new_size);
                Ok(NonNull::new(slice).ok_or(AllocError)?)
            } else {
                let new_ptr = self.allocate(new_layout)?;
                let to = new_ptr.as_ptr() as *mut u8;
                core::ptr::copy_nonoverlapping(old_ptr.as_ptr(), to, old_layout.size());
                self.deallocate(old_ptr, old_layout);
                Ok(new_ptr)
            }
        }

        unsafe fn grow_zeroed(
            &self,
            old_ptr: NonNull<u8>,
            old_layout: Layout,
            new_layout: Layout,
        ) -> Result<NonNull<[u8]>, AllocError> {
            let alloc = self.grow(old_ptr, old_layout, new_layout)?;
            let to_zero_part = &raw mut (*alloc.as_ptr())[old_layout.size()..];
            write_zeroes(to_zero_part);
            Ok(alloc)
        }

        unsafe fn shrink(
            &self,
            old_ptr: NonNull<u8>,
            old_layout: Layout,
            new_layout: Layout,
        ) -> Result<NonNull<[u8]>, AllocError> {
            let new_size = new_layout.size();
            if old_layout.align() == new_layout.align() {
                unsafe { core::hint::assert_unchecked(new_size <= old_layout.size()) };
                let ptr = alloc::alloc::realloc(old_ptr.as_ptr(), old_layout, new_size);
                let slice = core::ptr::slice_from_raw_parts_mut(ptr, new_size);
                Ok(NonNull::new(slice).ok_or(AllocError)?)
            } else {
                let new_ptr = self.allocate(new_layout)?;
                let to = new_ptr.as_ptr() as *mut u8;
                core::ptr::copy_nonoverlapping(old_ptr.as_ptr(), to, new_size);
                self.deallocate(old_ptr, old_layout);
                Ok(new_ptr)
            }
        }
    }
}
#[cfg(not(feature = "allocator-api"))]
pub use shim::{AllocError, Allocator, Global};
