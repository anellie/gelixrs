# Module System

In gelix, every file is automatically a module. 
Modules are used to namespace and organize your code.

For example, given the following directory structure:

```
my_project/
├── main.gel
└── animals/
    ├── cat.gel
    ├── dog.gel
    └── bird.gel
```

Assuming you have a `main` function in `main.gel`, you can compile the entire
set of modules by calling the compiler on the `my_project` directory.

If you need to use code from another module, you can import it like so:

```java
// Importing everything in cat.gel in the previous example (Wildcard import):
import my_project/animals/cat/+

// Importing only a single declaration, for example a class "Cat" in cat.gel:
import my_project/animals/cat/Cat
```

Note that it is not possible to import modules right now.

#### module.gel

By naming a file `module.gel`, it will be used as the module of the directory it is in.

For example:

```
my_project/
└── my_module/
    ├── module.gel
    └── other.gel
```

This would result in `my_project/my_module` containing everything in `module.gel`, allowing
importing as `import my_project/my_module/+`, while still permitting submodules 
(like `my_project/my_module/other` in this example).

### Export

Additionally to `import`, gelix also has the `export` statement. It does
the same as `import`, but also pulls the imported declaration into the module,
allowing other modules to import it.

This is useful for easier organization, particularly for libraries.

```java
// In code/long/module/path.gel:
class Example { ... }

// In code/short.gel:
export code/long/module/path/Example

// In another module:
import code/short/Example
```

### The Standard Library

Gelix ships with its standard library by default, which lives at the `std` module and
its children.

By default, all modules automatically import `std/prelude/+`, which is also where
things like the `String` datatype or `print` function reside in.

The standard library contains many useful components to ease development.
[Check it out!](https://gitea.angm.xyz/felix/gelixrs/src/branch/master/std)