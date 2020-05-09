# Looping

Lastly, a look at looping constructs in gelix.

Other than most C-like languages, gelix only has `for` expressions that cover
all iteration and looping needs.

### Conditional Looping

To loop based on a condition (think `while`), simply use for:

```java
var i = 0
for (i < 5) {
    print(i)
    i = i + 1
}
```

This will print `0 1 2 3 4`.

### Iterating Over an Integer Range

Since the above use case of iterating over a range of numbers is rather common,
gelix has special shortcut syntax for it:

```java
for (i from 0 to 5) print(i)
```

This code is equivalent to the conditional looping example above, but much more concise.

### Iterators (Not implemented yet)

Some data types like arrays implement the `Iterator<I>` interface 
(you will learn more about interfaces later).

To iterate a type that implements `Iterator`, you can once again use shortcut syntax,
like here with an integer array:

```java
val values = [12, 45, 36, 2]
for (value in values) print(value)
```

This will expectedly output `12 45 36 2`.

### For as Expression

As you could probably guess, `for` can also be an expression evaluating a value.

Here are the conditions:
- An else branch is present
- Both branches evalute to a value of the same type.
- All `break` expressions give a value of the same type (see below)

The else branch is executed whenever the `for` body never actually runs.
If it does run, the value is the value of the body on last execution.

```java
// This evaluates to 4
for (i from 0 to 5) {
    i
} else 0

// This evaluates to false
for (false) true else false

// This always evaluates to None due to the missing else branch
for (condition) { /* [...] */ }

// As does this, due to mismatched types
for (i in [1, 3, 2]) i else false
```

### Loop Control Flow: Break

Inside loops, you have another expression you probably know about from other
programming languages: `break`.

Once hit, a break expression will immediately terminate the loop completely
and continue execution after the loop.

To allow `break` to work with "for as expression", a break can have an optional value:

```java
// This prints "0 1" and evaluates to 2
for (i from 0 to 5) {
    if (i == 2) break i
    else print(i)
} else 255

// This causes a compile error, as the "break" expression
// must match the type of the loop or omit the value to
// make the loop return None:
for (i from 0 to 5) {
    if (i == 2) break "NaN" // Removing '"NaN"' would make this compile
    else print(i)
} else 255
```

