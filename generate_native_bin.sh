#!/bin/bash

# Generate bitcode
cargo run "$@"

# Compile stdlib
cd ./stdlib/
cargo build --release

# Compile bitcode to binary
# TODO: Figure out how to re-enable PIE
cd ..
llc out.ll
clang out.s -no-pie -L ./stdlib/target/release/ -l stdlib

# Go!
LD_LIBRARY_PATH=./stdlib/target/release ./a.out

# Clean up
rm out.bc out.ll out.s a.out