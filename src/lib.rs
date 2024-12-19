#![doc = include_str!("../README.md")]
//! ## Available features
//! - `allocator-api`: Requires nightly and enables allocating with a specific [Allocator].
//!
//! [Allocator]: https://doc.rust-lang.org/std/alloc/trait.Allocator.html
#![no_std]
#![cfg_attr(feature = "allocator-api", feature(allocator_api))]
#![warn(missing_docs)]

extern crate alloc;

mod alloc_shim;

mod r#impl;
pub use r#impl::Allocation;
mod std_conversions;
pub use std_conversions::{BoxConversionError, VecConversionError};

#[cfg(test)]
mod test;
