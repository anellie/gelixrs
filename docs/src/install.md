# Installation

### Dependencies

Before installing `gelixrs`, please ensure you have the following dependencies 
installed on your system:
- The `clang` compiler
- A recent version of the LLVM library (8.0.0+)
- A nightly rust compiler

### Downloading the source

Start by cloning the repository:

``` git clone https://gitea.angm.xyz/felix/gelixrs.git ```

Alternatively, head to the [repo](https://gitea.angm.xyz/felix/gelixrs)
in your browser and download a ZIP of the archive.

### Compiling and installing

To compile and install the compiler, simply run:

```bash
# Compile with optimizations
cargo build --release

# Copy to a location in your $PATH:
cp ./target/release/gelixrs ~/.local/bin/gelix
# Alternatively, to install system-wide:
sudo cp ./target/release/gelixrs /usr/local/bin/gelix

# Also copy the standard library to its path:
cp std .local/share/gelix-std
# Alternatively, to install system-wide:
sudo cp std /usr/local/lib/gelix-std
```

You should now be able to invoke the compiler with `gelix`.