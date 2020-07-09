# Classes

Like many other languages, gelix supports classes and object-oriented programming.

To define a class, use the `class` keyword:

```java
class Cat {
    // Class members
    val name = "Lily"
    var age = 7

    // Methods
    func meow() {
        print("Meow!")
    }

    func aged() {
        // The class can be accessed using "this"
        this.age = this.age + 1
    }

    func older(other: Cat) -> bool {
        this.age > other.age
    }
}
```

#### Instantiation

Now that you have created a class, you can create an instance by calling it like a function:

```java
func main() {
    val cat = Cat()
    print(cat.age) // 7
    cat.meow()
}
```

#### Constructors

In the `Cat` example, gelix automatically created a constructor with no arguments.
This happens if:
- You do not define any constructors yourself
- All class members have default values

If these conditions are not met or you need to initialize your class somehow, you can
create your own constructor:

```java
class Cat {
    val name: String
    var age: i64

    // Constructors are created by the "construct" keyword:
    construct(name: String, age: i64) {
        this.name = name
        this.age = age
    }

    // If you want to map an argument to a class memeber, use this
    // shorthand syntax:
    construct(name, age)
    // A constructor body can be omitted.
}
```

Note that it is not valid for a constructor to leave a class member
uninitialized - if it does, gelix will produce a compile error.

To call your own constructor, simply call the class with fitting arguments:

```java
val cat = Cat("Mitch", 4)
print(cat.age) // 4
cat.meow()
```

With that, you now know all there is to know about classes!