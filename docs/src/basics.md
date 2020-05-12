# Basics

Now that you have gotten a first taste of gelix, 
it's time to look at some of the basic features of the language.

## Functions

As you already saw in the Hello World example, simple functions in gelix are defined like this:

```java
func main() {
    // [...]
}
```

If you want to add a return type, use `->`:
```java
func hello() -> String {
    // Because blocks automatically produce their last expression
    // as a value, this is sufficient:
    "Hello!"
    // If you want to return from your function early, you could 
    // use the return keyword like this:
    // return "Hello!"
}
```

This is how you add parameters to your function:

```java
func add(a: i64, b: i64) -> i64 { // i64 is the built-in 64-bit signed int type
    a + b
}
```

To call your function, you can simply use regular C-style syntax:

```java
add(2, 7) // returns 9
```

Since the above `add` function is a little verbose, you can use single expression
function syntax for a smaller function:

```java
// Equal to the add function above
func add(a: i64, b: i64) -> i64 = a + b
```

Lastly, note that `main` is a special function name - you may not have more than one main function
in your code. `main` must not have any parameters or a return type. 
It is the function that is called when your program starts.

## Variables

To define a variable inside functions, you have 2 options:
- `var` will define a regular variable
- `val` will define an immutable variable that may not be reassigned

Here is an example:
```java
func main() {
    var number = 5
    val name = "gelix"

    number = number + 4 // Valid!
    // name = "oh no!"  // Would produce a compile error
    // number = "NaN"   // Not valid: gelix is statically typed

    print(number)
    print(name)
}
```
