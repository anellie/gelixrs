# When Expressions

Next, gelix's equivalent to `switch` in many languages: when expressions. 

The syntax looks like this:

```java
// Will print "something else"
when (5) {
    3 -> print("three")
    2 -> print("two")
    else -> print("something else")
}
```

When expressions take a value followed by a list of cases. It will
compare the value to cases in the order they are written and execute the branch
that equals the given value - if none do, it will execute the (optional) else branch.

#### When as Expression

Just like if expressions, when can also return the value of a branch.

The conditions are similar to if:
- An else branch is present OR all possible values are covered
- All branches evalute to a value of the same type.

If they are not met, the when expression will evalute to the `None` singleton value instead.

```java
var a = when (5) {
    3 -> "three"
    2 -> "two"
    else -> "else"
}
print(a) // prints "else"

// Both of these will evalute to None, as not all conditions are met:
when (4) { // Missing "else"
    3 -> "three"
    2 -> "two"
}       

when (4) { // Differing types
    3 -> "three"
    2 -> "two"
    else -> 0
}
```