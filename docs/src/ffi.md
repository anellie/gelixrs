# FFI

Currently, gelix only has the `extern` keyword to indicate a function
is defined during linking:

```java
// libc puts
extern func puts(a: i8)
```

A proper FFI is not yet implemented, but planned.