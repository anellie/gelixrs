# Closures

Like most other modern languages, gelix implements closures which allow treating
a function like data and using it for advanced behavior.

To begin with, as always a simple example:

```java
val a = 4
val add = (b: i64): i64 -> a + b
print(add(5)) // 9
print(add(6)) // 10
```

As you can see, closures can capture variables (here, `a` is captured) and can be called
like regular functions.

The real power of closures is using them as function arguments:

```java
func main() {
    // Type inference still TODO
   numbers((i: i64): i64 -> i * 4) // 0 4 8 12
   numbers((i: i64): i64 -> i + 2) // 2 3 4 5
}

func numbers(fn: (i64): i64) {
    for (num in 0 to 4) print(fn(num))
}
```