# Enums

The most powerful data type in gelix is enums.

Compared to simple enums in languages like Java or C++, gelix enums can contain data,
which makes them incredibly powerful.

To begin with, have an example of an enum definition:

```java
enum Value {
    // Just like classes, enums can have members and methods:    
    val name = "gelix"

    func print_name() {
        print(this.name)
    }
    
    // Other than classes, enums may not have constructors.
    // Instead, they have cases:
    Int {
        // Each case can also have its own additional members and methods:
        val int: i64

        func add_to(other: i64) {
            this.int = this.int + other
        }

        // More importantly, a case can contain constructors:
        construct(int)
    }

    // Since the above can be a bit verbose, a case with just members
    // and a constructor taking values for these members can look like this:
    Bool(val b: bool) 

    // Leaving the case empty entirely makes it behave like a simple Java-style enum:
    None 
}
```

Based on this example, you can hopefully see what makes enums so powerful - they
give the ability to share behavior between different data (cases) without
relying on any kind of reflection or dynamic behavior - enums compile to 
entirely static code, with no dynamic dispatch or similar like class inheritance
would result in.

### Using Enums

Now that you have defined an enum, here is how to use it:

```java
func main() {
    val a = Value:Bool(true)
    val b = Value:Int(23)
    print_values(a, b)
}

// Note that enum cases are also full types by themselves and can be used as such!
func print_values(a: Value, b: Value:Int) {
    print(b.int)
    print(a.name)

    // You can use if or when to switch on enum cases:
    val string = when (a) {
        Value:Int -> a.int.to_string()
        Value:Bool -> a.b.to_string()
        Value:None -> "None"
    }
    // Since all cases are covered, this will evalute to a string!
}
```

### Simple Cases

Lastly, sometimes when instancing emum cases you need to omit the `()`. This is the case when:
- The enum itself does not have any members
- The case is empty (like `Value:None` above)

When this is the case, simply omit the parenthesis:

```java
enum Maybe {
    case Some(val a: String) 
    case None
}

func main() {
    val a = Maybe:None
    if (a is Maybe:None) print("OK!")
}
```
