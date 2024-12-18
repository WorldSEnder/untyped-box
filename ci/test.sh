#! /bin/sh
set -e

cargo check "$@"
cargo test "$@"
