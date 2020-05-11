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

### Generic Bounds

While generics on their own are quite useful, you sometimes want to restrict the 
types you can use for generic arguments. For this, gelix has generic bounds.

These bounds allow you to set an interface a type must implement.

```java
// Imports are discussed in more detail later.
import std/string/ToString

// Generic bounds go after the name, separated by a colon:
func say<T: ToString>(a: T) {
    // (This works because strings allow adding implementors of ToString to them.)
    print("it says: " + a)
}

class A {}

func main() {
    say("Hello!")
    say(34)

    // This would cause a compile error, as `A` does not implement ToString:
    say(A())
}
``` 

Additionally to interface bounds, there are also markers that can be used as 
generic bounds, like `Primitive` or `Class`. See [here](gen_markers.md) for a full list.

This is basically all there is to gelix generics - because gelix compiles down to machine code,
generic type instances are considered entirely different and behave closer to C++ templates
than to generics like Java's.