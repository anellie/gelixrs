# Memory Management

Memory management in gelix is mostly automatic. However, gelix's type system differentiates
values of a given type based on their origin (stack or heap).

This allows for better performance and fine-grained control of where a value resides,
at a slight cost of type system complexity.

## Different types of values

There are 4 different types of values in gelix, based on where they are located:

#### Weak Reference

The default type for a value is a weak reference. A weak reference is a pointer to a value
that resides either on the heap or the stack and is simply the name of the type.

Due to the fact that the value can be on the stack, usage is restricted. A weak reference may
not leave a function that uses it, which means that you cannot:
- Assign the value to a field, as the field may outlive the value
- Return a weak reference from a function, as that would outlive the value

```java
func main() {
    a(Array())
}

// Simply the type name without any extras is a weak reference
func a(a: Array<i64>) {}

class A {
    val a: Array<i64>
    construct(a)
}

// This would be a compile error: You cannot assign a weak reference to a field.
func b(b: Array<i64>) {
    A(b)
}
```

#### Strong Reference

The second type is a strong reference. A strong reference is also a pointer, except it is
guaranteed to reside on the heap. SRs are always reference counted and automatically 
garbage collected.

Compared to WRs, SRs can be used with no restrictions. They are written with a `&` in front,
for example `&String`.

Lastly, SRs will automatically cast to WRs.

```java
// The above example would've been a compile error with WRs, but SRs make it possible
func b(b: &Array<i64>) {
    A(b)
}
```


#### Direct values

Direct values are not pointers, but instead values directly. This has a few implications:
- When used as function parameter, the argument is copied on every call
- When used as a field, the value is part of the parent data structure instead of a pointer
- A direct value cannot be used as either WR or SR.
- SRs and WRs will automatically cast to direct values

To use direct values, put a `~` in front, like `~String`.

```java
class A {
    // Because of the type, the "A" datatype is one block in memory
    // instead of being split across the heap like it would be with pointers
    val a: ~String = "hmm"
}

func b(a: ~A) {
    // The value can be used like normal:
    print(a.a)

    // If you need to promote the value to a strong reference, you can use
    // std/memory/promote:
    promote(a)
    // This will allocate on the heap and copy the value.
}
```

#### Raw Pointers

Lastly, gelix also offers you raw pointers. You should in general refrain from using them
unless absolutely necessary, as they are rarely needed outside of C interop.

To use raw pointers, put a `*` in front, like `*String`.

Note that the above will actually be a double pointer, since `String` is itself
already a pointer. If you want a raw single pointer, combine raw pointers with direct
values, like `*~String`.

Compared to references, raw pointers allow direct modification by using the functions
provided in the `std/memory` module. Use `std/ptr/allocate` to obtain a raw pointer.


### Creating new values

You already saw the constructor syntax for classes in the last chapter. Calling a data structure
directly will simply create a weak reference on the stack.

If you want to create a strong reference on the heap, put the `new` keyword before it:

```java
// Weak reference
Array<i64>()

// Strong reference
new Array<i64>()
```

