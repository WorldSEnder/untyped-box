use core::{alloc::Layout, any::type_name, mem::MaybeUninit, ptr::NonNull};

use crate::alloc_shim::{AllocError, Allocator, Global};

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
fn allocate_zeroed(
    alloc: &impl Allocator,
    layout: Layout,
) -> Result<(NonNull<u8>, Layout), AllocError> {
    let ptr = alloc.allocate_zeroed(layout)?;
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
unsafe fn grow_zeroed(
    alloc: &impl Allocator,
    ptr: NonNull<u8>,
    old_layout: Layout,
    new_layout: Layout,
) -> Result<(NonNull<u8>, Layout), AllocError> {
    let ptr = alloc.grow_zeroed(ptr, old_layout, new_layout)?;
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
    pub fn zeroed(layout: Layout) -> Self {
        Self::zeroed_in(layout, Global)
    }
    pub fn into_parts(self) -> (NonNull<u8>, Layout) {
        let (ptr, layout, _) = Self::into_parts_with_alloc(self);
        (ptr, layout)
    }
    /// # Safety
    /// Needs to provide values returned from a previous call to [Self::into_parts].
    pub unsafe fn from_parts(ptr: NonNull<u8>, layout: Layout) -> Self {
        Self::from_parts_in(ptr, layout, Global)
    }
}
/// Common methods
impl<A: Allocator> Allocation<A> {
    pub fn as_ptr<T>(&self) -> NonNull<T> {
        self.ptr.cast()
    }
    pub fn as_uninit_ref<T>(&self) -> &MaybeUninit<T> {
        assert!(
            self.layout.size() >= size_of::<T>(),
            "allocation too small to represent a {}",
            type_name::<T>()
        );
        assert!(
            self.layout.align() >= align_of::<T>(),
            "allocation not aligned for a {}",
            type_name::<T>()
        );
        unsafe { &*self.ptr.as_ptr().cast() }
    }
    pub fn as_uninit_mut<T>(&mut self) -> &mut MaybeUninit<T> {
        assert!(
            self.layout.size() >= size_of::<T>(),
            "allocation too small to represent a {}",
            type_name::<T>()
        );
        assert!(
            self.layout.align() >= align_of::<T>(),
            "allocation not aligned for a {}",
            type_name::<T>()
        );
        unsafe { &mut *self.ptr.as_ptr().cast() }
    }
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
    pub fn realloc_zeroed(&mut self, new_layout: Layout) {
        let () = self
            .try_realloc_zeroed(new_layout)
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
    pub fn zeroed_in(layout: Layout, alloc: A) -> Self {
        Self::try_zeroed_in(layout, alloc)
            .unwrap_or_else(|AllocError| alloc::alloc::handle_alloc_error(layout))
    }
    pub fn try_zeroed_in(layout: Layout, alloc: A) -> Result<Self, AllocError> {
        let (ptr, layout) = allocate_zeroed(&alloc, layout)?;
        Ok(Self { ptr, layout, alloc })
    }
    pub fn into_parts_with_alloc(self) -> (NonNull<u8>, Layout, A) {
        let me = core::mem::ManuallyDrop::new(self);
        let alloc = unsafe { core::ptr::read(&me.alloc) };
        (me.ptr, me.layout, alloc)
    }
    /// # Safety
    /// Needs to provide values returned from a previous call to [Self::into_parts_with_alloc].
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
    pub fn try_realloc_zeroed(&mut self, new_layout: Layout) -> Result<(), AllocError> {
        if new_layout == self.layout {
            return Ok(());
        }
        // Prefer grow to shrink when all we do is change alignment
        if new_layout.size() >= self.layout.size() {
            (self.ptr, self.layout) =
                unsafe { grow_zeroed(&self.alloc, self.ptr, self.layout, new_layout)? };
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