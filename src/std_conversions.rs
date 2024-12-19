use core::{alloc::Layout, mem::MaybeUninit, ptr::NonNull};

use alloc::{boxed::Box, vec::Vec};

use crate::{alloc_shim::Allocator, Allocation};

#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum BoxConversionError {
    LayoutMismatch { expected: Layout, allocated: Layout },
}

impl BoxConversionError {
    fn layout_mismatch(expected: Layout, allocated: Layout) -> Self {
        Self::LayoutMismatch {
            expected,
            allocated,
        }
    }
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum VecConversionError {
    AlignMismatch {
        expected: usize,
        allocated: usize,
    },
    SlackCapacity {
        element_size: usize,
        allocated: usize,
    },
    ZeroSizedElements,
}

impl VecConversionError {
    fn align_mismatch(expected: usize, allocated: usize) -> Self {
        Self::AlignMismatch {
            expected,
            allocated,
        }
    }
    fn slack_capacity(element_size: usize, allocated: usize) -> Self {
        Self::SlackCapacity {
            element_size,
            allocated,
        }
    }
    fn zero_sized_elements() -> Self {
        Self::ZeroSizedElements
    }
}

// we can NOT write
// impl<T, A: Allocator> TryFrom<crate::Allocation<A>> for Box<MaybeUninit<T>, A> {}
// since   ^^^^^^^^^^^^ this is uncovered generic argument               here -^
// Hence, we only support the conversion into the global allocator via trait.
// THIS IS STUPID!
impl<T> TryFrom<crate::Allocation> for Box<MaybeUninit<T>> {
    type Error = BoxConversionError;
    fn try_from(alloc: crate::Allocation) -> Result<Self, Self::Error> {
        alloc.try_into_box::<T>()
    }
}

fn check_box_layout<A: Allocator, T>(allocation: &Allocation<A>) -> Result<(), BoxConversionError> {
    let expected = Layout::new::<T>();
    let actual = allocation.layout();
    if expected != actual {
        return Err(BoxConversionError::layout_mismatch(expected, actual));
    }
    Ok(())
}
// TODO: conversion for unsized box/pointer metadata
// TODO: conversion to ThinBox?

fn check_vec_layout<A: Allocator, T>(
    allocation: &Allocation<A>,
) -> Result<usize, VecConversionError> {
    let expected = Layout::new::<T>();
    let actual = allocation.layout();
    let element_align = expected.align();
    let alloc_align = actual.align();
    if element_align != alloc_align {
        return Err(VecConversionError::align_mismatch(
            element_align,
            alloc_align,
        ));
    }
    let element_size = expected.size();
    let byte_capacity = actual.size();
    if (element_size == 0 && byte_capacity != 0) || byte_capacity % element_size != 0 {
        return Err(VecConversionError::slack_capacity(
            element_size,
            byte_capacity,
        ));
    }
    if element_size == 0 {
        // Can not determine a capacity.
        // We can not make up ZSTs on the spot, so a capacity of 0 makes sense.
        // TODO: let the user provide a capacity hint?
        return Err(VecConversionError::zero_sized_elements());
    }

    let element_capacity = byte_capacity / element_size;
    debug_assert!(byte_capacity == element_size * element_capacity);
    Ok(element_capacity)
}

impl<T> TryFrom<crate::Allocation> for Vec<T> {
    type Error = VecConversionError;

    fn try_from(value: crate::Allocation) -> Result<Self, Self::Error> {
        value.try_into_vec()
    }
}

#[cfg(feature = "allocator-api")]
mod alloc_allocator_api {
    macro_rules! box_to_parts {
        ($value:ident) => {
            Box::into_raw_with_allocator($value)
        };
    }
    macro_rules! vec_to_parts {
        ($vec:ident) => {
            Vec::into_raw_parts_with_alloc($vec)
        };
    }
    macro_rules! box_from_parts {
        ($ptr:expr, $alloc:expr) => {{
            Box::from_raw_in($ptr, $alloc)
        }};
    }
    macro_rules! vec_from_parts {
        ($ptr:expr, $cap:expr, $alloc:expr) => {{
            Vec::from_raw_parts_in($ptr, 0, $cap, $alloc)
        }};
    }
    macro_rules! allocation_impl {
        ( $( $imp:tt )* ) => {
            type ABox<T, A> = alloc::boxed::Box<T, A>;
            type AVec<T, A> = alloc::vec::Vec<T, A>;
            impl<A: Allocator> crate::Allocation<A> {
                $( $imp )*
            }
        };
    }
    macro_rules! from_box_impl {
        ( $( #[doc = $doc:literal] )*
          struct DocAnchor;
          $( $imp:tt )*
        ) => {
            $( #[doc = $doc ] )*
            impl<T: ?Sized, A: Allocator> From<Box<T, A>> for crate::Allocation<A> {
                $( $imp )*
            }
        };
    }
    macro_rules! from_vec_impl {
        ( $( #[doc = $doc:literal] )*
          struct DocAnchor;
          $( $imp:tt )*
        ) => {
            $( #[doc = $doc ] )*
            impl<T, A: Allocator> From<Vec<T, A>> for crate::Allocation<A> {
                $( $imp )*
            }
        };
    }
    pub(super) use allocation_impl;
    pub(super) use box_from_parts;
    pub(super) use box_to_parts;
    pub(super) use from_box_impl;
    pub(super) use from_vec_impl;
    pub(super) use vec_from_parts;
    pub(super) use vec_to_parts;
}

#[cfg(not(feature = "allocator-api"))]
mod alloc_no_allocator_api {
    macro_rules! box_to_parts {
        ($value:ident) => {
            (Box::into_raw($value), $crate::alloc_shim::Global)
        };
    }

    macro_rules! vec_to_parts {
        ($vec:ident) => {{
            let mut $vec = core::mem::ManuallyDrop::new($vec);
            // TODO: wait for feature(vec_into_raw_parts)
            (
                $vec.as_mut_ptr(),
                $vec.len(),
                $vec.capacity(),
                $crate::alloc_shim::Global,
            )
        }};
    }

    macro_rules! box_from_parts {
        ($ptr:expr, $alloc:expr) => {{
            let _: $crate::alloc_shim::Global = $alloc;
            alloc::boxed::Box::from_raw($ptr)
        }};
    }
    macro_rules! vec_from_parts {
        ($ptr:expr, $cap:expr, $alloc:expr) => {{
            let _: $crate::alloc_shim::Global = $alloc;
            alloc::vec::Vec::from_raw_parts($ptr, 0, $cap)
        }};
    }

    macro_rules! allocation_impl {
        ( $( $imp:tt )* ) => {
            pub trait UseA<A> { type This: ?Sized; }
            impl<A, T: ?Sized> UseA<A> for T { type This = Self; }
            type ABox<T, A> = <alloc::boxed::Box<T> as UseA<A>>::This;
            type AVec<T, A> = <alloc::vec::Vec<T> as UseA<A>>::This;

            type A = $crate::alloc_shim::Global;
            impl<> crate::Allocation<> {
                $( $imp )*
            }
        };
    }
    macro_rules! from_box_impl {
        ( $( #[doc = $doc:literal] )*
          struct DocAnchor;
          $( $imp:tt )*
        ) => {
            $( #[doc = $doc ] )*
            impl<T: ?Sized> From<Box<T>> for crate::Allocation {
                $( $imp )*
            }
        };
    }
    macro_rules! from_vec_impl {
        ( $( #[doc = $doc:literal] )*
          struct DocAnchor;
          $( $imp:tt )*
        ) => {
            $( #[doc = $doc ] )*
            impl<T> From<Vec<T>> for crate::Allocation {
                $( $imp )*
            }
        };
    }
    pub(super) use allocation_impl;
    pub(super) use box_from_parts;
    pub(super) use box_to_parts;
    pub(super) use from_box_impl;
    pub(super) use from_vec_impl;
    pub(super) use vec_from_parts;
    pub(super) use vec_to_parts;
}

#[cfg(feature = "allocator-api")]
use alloc_allocator_api as api_impl;
#[cfg(not(feature = "allocator-api"))]
use alloc_no_allocator_api as api_impl;

api_impl::allocation_impl! {
    /// Convert the allocation into a box.
    ///
    /// This fails if the allocated layout does not match the requested type. The value might not be initialized,
    /// use [`Box::assume_init`] in case you have initialized the memory of this allocation correctly.
    ///
    /// See also the opposite conversion `Allocation as From<Box<_>>`.
    // TODO: add intro-doc link to `<Allocation as From<Box<_>>>`
    pub fn try_into_box<T>(self) -> Result<ABox<MaybeUninit<T>, A>, BoxConversionError> {
        let () = check_box_layout::<_, T>(&self)?;
        // Commit to the conversion
        let (ptr, _, alloc) = self.into_parts_with_alloc();
        let ptr = ptr.as_ptr().cast();
        // SAFETY:
        Ok(unsafe { api_impl::box_from_parts!(ptr, alloc) })
    }

    /// Convert the allocation into a [`Vec`].
    ///
    /// This fails if the allocated size is not a multiple of the requested element size, or if the element type is zero-sized.
    /// For the latter case, the capacity of the `Vec` would be ambiguous.
    ///
    /// The length of the returned vec is always set to `0` and has to be resized manually with [`Vec::set_len`].
    ///
    /// See also the opposite conversion `Allocation as From<Vec<_>>`.
    // TODO: add intro-doc link to `<Allocation as From<Vec<_>>>`
    pub fn try_into_vec<T>(self) -> Result<AVec<T, A>, VecConversionError> {
        let capacity = check_vec_layout::<_, T>(&self)?;
        let (ptr, _, alloc) = self.into_parts_with_alloc();
        let ptr = ptr.as_ptr().cast();
        Ok(unsafe { api_impl::vec_from_parts!(ptr, capacity, alloc) })
    }
}

// This has to appear side-by-side with allocation_impl because it relies on `A` and `ABox` to be defined

api_impl::from_box_impl! {
    /// The value in the box will not be dropped, as if passed to [`forget`](core::mem::forget).
    /// Use the inverse (fallible) conversion to recover the value.
    ///
    /// ```
    /// # use std::mem::MaybeUninit;
    /// # use untyped_box::Allocation;
    /// let boxed = Box::new(42);
    /// let alloc: Allocation = boxed.into();
    /// let boxed = alloc.try_into_box::<u32>().unwrap();
    /// let boxed = unsafe { boxed.assume_init() };
    /// assert_eq!(*boxed, 42);
    /// ```
    struct DocAnchor;

    fn from(value: ABox<T, A>) -> Self {
        let layout = Layout::for_value(&*value);
        let (ptr, alloc) = api_impl::box_to_parts!(value);
        let ptr = unsafe { NonNull::new_unchecked(ptr) };
        unsafe { Self::from_parts_in(ptr.cast(), layout, alloc) }
    }
}

// This has to appear side-by-side with allocation_impl because it relies on `A` and `ABox` to be defined

api_impl::from_vec_impl! {
    /// The values in the `Vec` will not be dropped, as if by a call to [`vec.set_len(0)`](Vec::set_len).
    ///
    /// ```
    /// # use std::mem::MaybeUninit;
    /// # use untyped_box::Allocation;
    /// let values = vec![42];
    /// let alloc: Allocation = values.into();
    /// let mut values = alloc.try_into_vec::<u32>().unwrap();
    /// unsafe { values.set_len(1) };
    /// assert_eq!(values, [42]);
    /// ```
    struct DocAnchor;

    fn from(value: AVec<T, A>) -> Self {
        let mut value = value;
        unsafe { value.set_len(0) };
        let layout = Layout::for_value(value.spare_capacity_mut());
        let (ptr, _, _, alloc) = api_impl::vec_to_parts!(value);
        let ptr = unsafe { NonNull::new_unchecked(ptr) };
        unsafe { Self::from_parts_in(ptr.cast(), layout, alloc) }
    }
}
