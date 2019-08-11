# gelixrs

A compiler for gelix, a simple programming language. Written in Rust.

It uses LLVM to output native binaries.

For more information on gelix, check out the docs directory for examples.

### Progress

The code is an alpha state. The compiler can output native code with LLVM.

However, no classes are implemented. Only functions currently exist, but external functions (FFI)
are possible.

### Build/Run

``` bash
# Build as release
cargo build --release

# Compile $file
gelixrs $file --output exec.out
```

### Code style

Follow the style used by rustfmt.
Max line length is 120.