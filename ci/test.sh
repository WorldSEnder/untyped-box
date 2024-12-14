#! /bin/sh
set -e

cargo test
cargo +nightly miri test
cargo +nightly miri test --features allocator-api
