# Nullable

Any type in gelix can also be made nullable by appending `?`. Doing so causes the
type to be able to hold an uninitialized value ("`null`"), requiring you 
to ensure it is not null before using it.


```java
func main() {
    print(addOrOne(4, 2)) // 6
    print(addOrOne(5, null)) // 6
}

// Add b to a, or add 1 if b is null.
func addOrOne(a: i64, b: i64?) -> i64 {
    val b = if (b == null) 1 else b // b is smart cast to 'i64' here
    return a + b
}
```

### Syntax sugar

To make the above easier, gelix offers the `a ?? b` operator. It returns `a` if
it is not null, or `b` if it is (Same as C#'s `??` and Kotlin's `?:`).

```java
func addOrOne(a: i64, b: i64?) -> i64 = a + (b ?? 1)
```

### Methods

In addition to `??`, gelix has null coalescing for fields or methods using `?.` instead
of `.`. If the callee is `null`, the result is null, otherwise, the method is called regularly.

```java
class A {
    val a: i64?
    
    construct(a)

    func hello() {
        print(if (a == null) "No." else "Hello!")
    }
}

func main() {
    var obj: A? = A(4)
    print(obj?.a) // 4
    obj = null
    print(obj?.a) // null since obj is null
    obj = A(null)
    print(obj?.a) // null since a is null
}
```