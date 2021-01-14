# Memory Management

Memory management in gelix is mostly automatic. However, gelix's type system differentiates
values based on how they're stored and copied.

This allows for better performance and fine-grained control of where a value resides,
at a slight cost of (optional) type system complexity.

## Different types of values

There are 2 different types of values in gelix:

### Reference Types

Reference types are types where creating a copy, for example by storing it
into a variable, only causes the *reference* to the value to be copied.

This is the same semantics as classes/objects in most mainstream OOP languages,
and classes are reference types by default:

```java
class MyClass {
    var a = 4
}

func main() {
    val m1 = MyClass()
    print(m1.a) // 4

    val m2 = m1 // Create a copy
    m1.a = 664 // Change field on m1

    // Because they're pointing to the same object, 
    // the field on m2 changes as well:
    print(m2.a) // 664 
}
```

In general, the following behaves like this:

- All classes and enums without the `value` modifier (shown later)
- All interfaces (value types are *boxed*, or encapsulated inside a reference type)
- All raw pointers (see below) and `extern` classes (explained in the [FFI chapter](ffi.md))

Except for `extern` classes and raw pointers, reference types are generally
heap-allocated and reference-counted.

#### Value Types

Value types are types where creating a copy actually copies the entire value.

This is done for:

- All primitive types (`bool`, all numeric types)
- Classes and enums with the `value` modifier

```java
value class Point {
    var x = 0
    var y = 0
}

func main() {
    val m1 = Point()
    print(m1.x) // 0

    val m2 = m1 // Create a copy
    m1.x = 664 // Change field on m1

    // Because creating a copy cloned the entire
    // value, they point to different objects:
    print(m2.x) // 0 
}
```

Being able to create your own value types is useful when you want to prevent heap allocation 
or having values scattered across the heap. 
Be warned however that big types (many members) will be quite slow with `value`.

**If you are unsure if your class should use `value`, it is recommended to avoid it.**

## Raw Pointers

Lastly, gelix also offers you raw pointers. You should in general refrain from using them
unless absolutely necessary, as they are rarely needed outside of C interop.

To use raw pointers, put a `*` in front, like `*String`.

Note that the above will actually be a double pointer, since `String` is itself
already a reference type and therefore a pointer. If you want a raw single pointer, 
combine raw pointers with the deref type operator, like `*~String`.

Compared to references, raw pointers allow direct modification by using the functions
provided in the `std/memory` and `std/ptr` modules. 
Use `std/ptr/allocate` to obtain a raw pointer.
