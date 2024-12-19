use core::{alloc::Layout, any::type_name, mem::MaybeUninit, ptr::NonNull};

use crate::alloc_shim::{AllocError, Allocator, Global};

/// An allocation is management representation of some allocated memory.
///
/// For the most part, this behaves like a lower-level (untyped) cousin of a `Box`.
/// The memory backing this allocation is deallocated when the allocation is dropped.
/// In contrast, no validity or initialization state of the memory is implied by
/// existance of an [Allocation].
pub struct Allocation<A: Allocator = Global> {
    // TODO: should be a Unique pointer!
    ptr: NonNull<u8>,
    layout: Layout,
    alloc: A,
}

// TODO: There is a bit of a mismatch here. In essence, we are losing information.
// For example, requesting an allocation for some `Layout::new::<T>()` that results in the allocator
// giving us more memory than we asked for might make later checks when trying to convert to a `Box`
// fail on size mismatch.
// We might have to blow up the allocation struct to reconstruct [Memory fitting] information.
// [Memory fitting]: https://doc.rust-lang.org/nightly/alloc/alloc/trait.Allocator.html#memory-fitting
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
    /// Allocate new memory for the given layout.
    ///
    /// The pointer backing the allocation is valid for reads and writes of `layout.size()` bytes and this
    /// memory region does not alias any other existing allocation.
    ///
    /// The pointer is guaranteed to be aligned to `layout.align()` but several systems align memory more
    /// lax when a small alignment is requested.
    ///
    /// Memory is not initialized or zeroed, try [`Self::zeroed`] instead.
    ///
    /// # Panics
    ///
    /// This calls [`alloc::alloc::handle_alloc_error`] when no memory could be allocated, which can panic.
    /// See [`Self::try_new_in`] for a version that returns an error instead.
    // Forwards to alloc, handles layout.size() == 0 with a dangling ptr
    pub fn new(layout: Layout) -> Self {
        Self::new_in(layout, Global)
    }
    /// Allocate new zeroed-out memory for the given layout.
    ///
    /// # Panics
    ///
    /// This calls [`alloc::alloc::handle_alloc_error`] when no memory could be allocated, which can panic.
    /// See [`Self::try_zeroed_in`] for a version that returns an error instead.
    pub fn zeroed(layout: Layout) -> Self {
        Self::zeroed_in(layout, Global)
    }
    /// Split the allocation into its raw parts.
    ///
    /// Deallocating the allocation is the responsibility of the caller. The returned
    /// pointer can be passed to [`alloc::alloc::dealloc`] if the returned layout indicates `size() > 0`.
    /// If the allocated memory is 0 sized, the pointer does not need to be deallocated.
    ///
    /// See also [`Self::into_parts_with_alloc`] for an allocator-aware version.
    pub fn into_parts(self) -> (NonNull<u8>, Layout) {
        let (ptr, layout, _) = Self::into_parts_with_alloc(self);
        (ptr, layout)
    }
    /// Constructs an [`Allocation`] from a pointer and layout information.
    ///
    /// # Safety
    ///
    /// The pointer must point to [*currently-allocated*] memory from the global allocator, and `layout`
    /// was used to allocate that memory.
    ///
    /// [*currently-allocated*]: Allocator#currently-allocated-memory
    pub unsafe fn from_parts(ptr: NonNull<u8>, layout: Layout) -> Self {
        Self::from_parts_in(ptr, layout, Global)
    }
}
/// Common methods
impl<A: Allocator> Allocation<A> {
    /// Gets a pointer to the allocation.
    ///
    /// The pointer is always aligned to the alignment of the layout indicated by [Self::layout] or the requested layout
    /// indicated on allocation, whichever is more strict.
    ///
    /// The pointer can be used to read and write memory in this allocation until it is [reallocated](Self::realloc),
    /// dropped or the memory is reclaimed manually (e.g. after converting [`into_parts`](Self::into_parts)).
    ///
    /// In particular, the pointer does not in itself materialize a reference to the underlying storage for the purpose of the aliasing model.
    pub fn as_ptr<T>(&self) -> NonNull<T> {
        self.ptr.cast()
    }
    /// View the underlying storage as a possibly uninitialized `T`.
    ///
    /// # Panics
    ///
    /// If the allocation is too small, or not aligned enough to contain a `T`.
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
    /// View the underlying storage as a possibly uninitialized `T`.
    ///
    /// # Panics
    ///
    /// If the allocation is too small, or not aligned enough to contain a `T`.
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
    /// View the allocation as a pointer to a slice of possibly uninitialized bytes.
    ///
    /// The caller is responsible for checking lifetimes when convert to a reference.
    ///
    /// The pointer can be used to read and write memory in this allocation until it is [reallocated](Self::realloc),
    /// dropped or the memory is reclaimed manually (e.g. after converting [`into_parts`](Self::into_parts)).
    ///
    /// Like [`as_ptr`](Self::as_ptr), this does not materialize a reference to the underlying storage for the purpose of the aliasing model.
    /// Hence, these two methods can be intermixed.
    pub fn as_slice(&self) -> NonNull<[MaybeUninit<u8>]> {
        let ptr = core::ptr::slice_from_raw_parts_mut(
            self.ptr.as_ptr().cast::<MaybeUninit<u8>>(),
            self.layout.size(),
        );
        unsafe { NonNull::new_unchecked(ptr) }
    }
    /// Reallocates memory to a new layout.
    ///
    /// If the newly requested layout is larger than the currently allocated layout, existing (possibly uninitialized) bytes are preserved.
    /// Newly allocated bytes are uninitialized.
    ///
    /// Any pointers to the managed memory are invalidated on return.
    ///
    /// # Panics
    ///
    /// This calls [`alloc::alloc::handle_alloc_error`] when no memory could be allocated, which can panic. In this case, pointers are still valid.
    /// See [`Self::try_realloc`] for a version that returns an error instead.
    // Calls either grow or shrink, compares against stored layout
    pub fn realloc(&mut self, new_layout: Layout) {
        let () = self
            .try_realloc(new_layout)
            .unwrap_or_else(|AllocError| alloc::alloc::handle_alloc_error(new_layout));
    }
    /// Reallocates memory to a new layout.
    ///
    /// If the newly requested layout is larger than the currently allocated layout, existing (possibly uninitialized) bytes are preserved.
    /// Newly allocated bytes are zeroed.
    ///
    /// Any pointers to the managed memory are invalidated on return.
    ///
    /// # Panics
    ///
    /// This calls [`alloc::alloc::handle_alloc_error`] when no memory could be allocated, which can panic. In this case, pointers are still valid.
    /// See [`Self::try_realloc_zeroed`] for a version that returns an error instead.
    pub fn realloc_zeroed(&mut self, new_layout: Layout) {
        let () = self
            .try_realloc_zeroed(new_layout)
            .unwrap_or_else(|AllocError| alloc::alloc::handle_alloc_error(new_layout));
    }
    /// Get the layout of the underlying allocation.
    ///
    /// This layout is guaranteed to be at least as large as previously requested from [`new`](Self::new) or [`realloc`](Self::realloc) and
    /// at least as strictly aligned, but might indicate more available memory.
    pub fn layout(&self) -> Layout {
        self.layout
    }
}
/// Methods using the allocator-api or shim
impl<A: Allocator> Allocation<A> {
    /// Allocate new memory for the given layout in a given allocator.
    ///
    /// The pointer backing the allocation is valid for reads and writes of `layout.size()` bytes and this
    /// memory region does not alias any other existing allocation.
    ///
    /// The pointer is guaranteed to be aligned to `layout.align()` but several systems align memory more
    /// lax when a small alignment is requested.
    ///
    /// Memory is not initialized or zeroed, try [`Self::zeroed_in`] instead.
    ///
    /// # Panics
    ///
    /// This calls [`alloc::alloc::handle_alloc_error`] when no memory could be allocated, which can panic.
    /// See [`Self::try_new_in`] for a version that returns an error instead.
    pub fn new_in(layout: Layout, alloc: A) -> Self {
        Self::try_new_in(layout, alloc)
            .unwrap_or_else(|AllocError| alloc::alloc::handle_alloc_error(layout))
    }
    /// Allocate new memory for the given layout in a given allocator.
    ///
    /// Returns an error when no memory could be allocated.
    pub fn try_new_in(layout: Layout, alloc: A) -> Result<Self, AllocError> {
        let (ptr, layout) = allocate(&alloc, layout)?;
        Ok(Self { ptr, layout, alloc })
    }
    /// Allocate new zeroed-out memory for the given layout in a given allocator.
    ///
    /// # Panics
    ///
    /// This calls [`alloc::alloc::handle_alloc_error`] when no memory could be allocated, which can panic.
    /// See [`Self::try_zeroed_in`] for a version that returns an error instead.
    pub fn zeroed_in(layout: Layout, alloc: A) -> Self {
        Self::try_zeroed_in(layout, alloc)
            .unwrap_or_else(|AllocError| alloc::alloc::handle_alloc_error(layout))
    }
    /// Allocate new zeroed-out memory for the given layout in a given allocator.
    ///
    /// Returns an error when no memory could be allocated.
    pub fn try_zeroed_in(layout: Layout, alloc: A) -> Result<Self, AllocError> {
        let (ptr, layout) = allocate_zeroed(&alloc, layout)?;
        Ok(Self { ptr, layout, alloc })
    }
    /// Split the allocation into its raw parts including the allocator.
    ///
    /// Deallocating the allocation is the responsibility of the caller. The returned
    /// pointer can be passed to `alloc.deallocate()`.
    pub fn into_parts_with_alloc(self) -> (NonNull<u8>, Layout, A) {
        let me = core::mem::ManuallyDrop::new(self);
        let alloc = unsafe { core::ptr::read(&me.alloc) };
        (me.ptr, me.layout, alloc)
    }
    /// Constructs an [`Allocation`] from a pointer and layout information in the given allocator.
    ///
    /// # Safety
    ///
    /// The pointer must point to [*currently-allocated*] memory from the given allocator, and `layout`
    /// [*fits*] that memory.
    ///
    /// [*currently-allocated*]: Allocator#currently-allocated-memory
    /// [*fits*]: Allocator#memory-fitting
    pub unsafe fn from_parts_in(ptr: NonNull<u8>, layout: Layout, alloc: A) -> Self {
        Self { ptr, layout, alloc }
    }
    /// Reallocates memory to a new layout.
    ///
    /// Returns an error when the memory could not be reallocated. In this case, any previously derived
    /// pointers remain valid and no memory is deallocated.
    ///
    /// # See also
    ///
    /// [`Self::realloc`] for more disuccion about the memory contents after reallocation.
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
    /// Reallocates memory to a new layout.
    ///
    /// Returns an error when the memory could not be reallocated. In this case, any previously derived
    /// pointers remain valid and no memory is deallocated.
    ///
    /// # See also
    ///
    /// [`Self::realloc_zeroed`] for more disuccion about the memory contents after reallocation.
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
