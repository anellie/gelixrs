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
    // To use a generic function, simply call it normally:
    print(echo(42)) // "42"
    // Gelix will automatically infer the generics from the arguments
    // you pass.

    // If gelix can't infer all types, you need to specify them 
    // explicitly or the compiler will throw an error.
    // To do so, use ::<>:
    print(echo::<bool>(true)) // "true"
    // (Would not be needed here, just an example!)

    val wrapper = Wrapper(2)
    print(wrapper.value) // "2"
    print(wrapper.echo("hello!")) // "hello!"
}

// When using generic types as function arguments:
func test(a: Wrapper<i64>) {
    print(a.value)
}
```

This is basically all there is to gelix generics - because gelix compiles down to machine code,
generic type instances are considered entirely different and behave closer to C++ templates
than to generics like Java's.