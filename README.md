# gelixrs

A compiler for gelix, a simple programming language. Written in Rust.

It uses the Inkwell LLVM wrapper to output native binaries.

For more information on gelix, check out the docs at [gelix.angm.xyz](https://gelix.angm.xyz).

Note that while gelix is not yet feature complete, all features described in the docs 
are implemented and should work.

### Dependencies

gelixrs depends on `llvm` and `clang` version `8.0`. You'll also need a nightly
rust compiler for some experimental rust features used.

### Compiling & Tests

To compile gelixrs, simply use cargo with `cargo build`.

To run the test suite, use `cargo test -- --nocapture` to prevent
the test runner output from being captured by cargo.

### Code style

Follow the style used by rustfmt.
