# Interfaces

Next, let's take a look at interfaces, which allow defining shared behavior
across types.

An interface may only contain methods, and is defined like so:

```java
interface ToString {
    func to_string() -> String
}
```

As you can see, an interface simply consists of method signatures with no body.

Since that is not very useful on its own, let's implement it:

```java
impl ToString for bool {
    func to_string() -> String {
        if (this) "true"
        else "false"
    }
}
```

As you can see in this example, interfaces are implemented using the `impl` keyword.
It's possible to implement interfaces for all types, including primitive ones!

### Using the Interface Type

Once you have an interface and implemented it, you can use it:

```java
func main() {
    print(stringify(false)) // "false"
}

func stringify(a: ToString) -> String {
    a.to_string()
}
```

Gelix will automatically cast implementors to the interface type, but you can also
explicitly cast using the `cast` function:

```java
// The "::" generics syntax will be further explained later on
cast::<ToString>(false)
```

### Casting Backwards

If you want to cast from an interface type back to the implementing type,
you can use the `is` operator:

```java
val iface = cast::<ToString>(false)
if (iface is bool) {
    // iface is now automatically cast to type "bool"
}

// This also works with when:
when (iface) {
    bool -> {
        // iface is now automatically cast to type "bool" 
    }
}
```