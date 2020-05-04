# Generics

Now that you know about gelix's data types, it's time for generics!

Generics are supported on all user-defined elements:
- Functions (and methods)
- Classes
- Interfaces
- Enums

Generics syntax is very similar between types, so have an example:

```java
// Generic parameters always go after the name:
func echo<T>(a: T) -> T {
    a
}

// For declarations:
class Wrapper<T> {
    val value: T

    construct(value)

    // Methods can also have generics
    func echo<O>(a: O) -> O {
        a
    }
}


func main() {
    // To instanciate any generic type, use ::<>:
    print(echo::<bool>(true)) // "true"
    // (Type inference is not implemented yet, but planned!)

    val wrapper = Wrapper::<i64>(2)
    print(wrapper.value) // "2"
    print(wrapper.echo::<String>("hello!")) // "hello!"
}

// When using generic types as function arguments, you omit the "::":
func test(a: Wrapper<i64>) {
    print(a.value)
}
```

This is basically all there is to gelix generics - because gelix compiles down to machine code,
generic type instances are considered entirely different and behave closer to C++ templates
than to generics like Java's.