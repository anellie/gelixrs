# gelixrs

A compiler for gelix, a simple programming language. Written in Rust.

It uses the Inkwell LLVM wrapper to output native binaries.

For more information on gelix, check out the docs directory for examples.

### Progress

The code is an alpha state. The compiler can output native code with LLVM.

Most of gelix's features are implemented. However, the compiler is not stable,
and the resulting binary should not be used outside of testing.

Also, the current master branch regularly has mayor breakage when refactoring code.

### Build/Run

``` bash
# Build as release
cargo build --release

# Compile $file
gelixrs $file -o exec.out
```

### Code style

Follow the style used by rustfmt.
