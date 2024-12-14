# untyped-box

A `Box<T>` represents a heap allocation of a value of type `T`. This crate provides an untyped heap allocation type `Allocation`.
This is useful to avoid monomorphizations on `T`, share code paths going through the allocator, while upholding safety invariants.
The allocator contract of the `unsafe` allocation methods is quite strict and easy to misuse.
This primitive can be used as a safe layer on top to avoid dealing with the allocation methods directly.
