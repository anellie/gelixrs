# FFI

Currently, gelix has the `extern` keyword that can be applied to 
functions and classes, as well as pointer and value types.

This allows for hopefully rather easy interop, mainly with C.

## Gelix Types at a Low Level

Gelix compiles to LLVM IR, which has a similar type system to that of C.
These types are in the contest of LLVM, but if you know C types, this will
mostly translate 1:1.

When using a class regularly, it's type is a pointer to an underlying struct.

This struct contains the following, in order:

- A i32 value used by the reference-counting garbage collector
- An `TypeInfo` pointer to static type information data for runtime reflection
- All user fields, in the order they were declared.

For example:

``` java
class T {
    val a = 3
    val b = 3.5
}

// Compiles to this LLVM IR type:
// %T = type { i32, *TypeInfo, i64, f64 }
```

While this format is good for standalone gelix, the additional fields make it hard to
interop with C. As a solution, gelix has the `extern` keyword on classes, which will:

- Make the struct only consist of the class members defined by the user
- *Disable* the GC on this object, relying on the programmer to free it with
`std/memory/free`
- *Disable* runtime reflection, making things like the `is` operator unavailable
- Easily allow defining C structs and using them in gelix code, making interop
seamless and zero-cost

For an example:

```java
extern class T {
    val a = 3
    val b = 3.5

    // Note that extern classes can still have methods and constructors!
    func b_sq() -> f64 = this.b * this.b
}

// Compiles to this LLVM IR type:
// %T = type { i64, f64 }

// You can create an object like normal:
val a = T()
// But you *need* to call free on it, otherwise your program will leak memory!
import std/memory/free
free(a)
// Note that this has the same use-after-free pitfalls as C - be careful!
```

As you can see, gelix offers quite good compability with C structs and easy interop.

## Functions

While C structs are nice, they won't get you far without C functions to use them.

Similarly to classes, putting the `extern` keyword changes a function definition:

- Function name will be unmodified and not namespaced by module
- Function may not have a body
- Function will be only declared in LLVM IR and defined during linking

Have an example:

On the C side:

```c
typedef struct Thing {
    int a,
    double b
}

char hello(Thing* a) {
    // [...]
}
```

Gelix:

```java
extern class Thing {
    val a = 0
    val b = 0.0
}

extern func hello(a: Thing) -> i8
```

As you can see by this example, structs/classes in gelix are a pointer by default.
If you need a double pointer, or a value, use `*` and `^`, respectively:

```java
extern func a_thing(a: ^Thing, b: Thing, c: *Thing)

// Translated to C:
void a_thing(Thing a, Thing* b, Thing** c) {}
```

This behaves a little different for primitive types:

```java
// "^" on a primitive does nothing
extern func a_thing(a: ^i8, b: i8, c: *i8)

// Translated to C:
void a_thing(char a, char b, char* c) {}
```

Lastly, a few notes:

- If you need to cast `Thing` to `^Thing`, use `std/memory/load_value`.
- If you need a `*Thing`, you can use `std/ptr/allocate`.


With this, you should hopefully be able to efficiently use C code in your
gelix application.