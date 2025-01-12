#![doc = include_str!("../README.md")]
//! ## Available features
//! - `nightly-std-conversions`: Requires nightly and enables additional conversions for `Box` and `Vec` types in std.
#![no_std]
#![cfg_attr(feature = "nightly-std-conversions", feature(allocator_api))]
#![warn(missing_docs)]

extern crate alloc;

mod alloc_shim;

mod r#impl;
pub use r#impl::Allocation;
mod std_conversions;
pub use std_conversions::{BoxConversionError, VecConversionError};

#[cfg(test)]
mod test;
