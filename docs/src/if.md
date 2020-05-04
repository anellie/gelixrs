# If Expressions

First of all, let's start with classic `if` expressions.

The syntax will probably look familiar:

```java
if (condition) {
    // [...]
} else {
    // [...]
}

// Short form:
if (condition) /* */ else /* */
```

Like you are probably used to, if expressions take a boolean condition and execute the expression
following it if the condition is true. If it is not, the optional `else` branch is executed instead.

#### If as Expression

A special feature of gelix is that there are no statements - only expressions. As a
result, an if expression will evaluate to the value of the branch that it executed.

Note that this is only the case if following conditions are met:
- An else branch is present
- Both branches evalute to a value of the same type.

If they are not met, the if expression will evalute to the `None` singleton value instead.

```java
var a = if (true) "nice" else "oh no"
print(a) // prints "nice"

// Both of these will evalute to None, as not all conditions are met:
if (false) "no"         // Missing "else"
if (true) 5 else "NaN"  // Differing types
```