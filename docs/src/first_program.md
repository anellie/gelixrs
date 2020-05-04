# Your First Gelix Program

Now that you have a gelix compiler installed, it is time to make your first program!

Start by making a new file called `helloworld.gel` and fill it with these contents:

```gelix
func main() {
    print("Hello World!")
}
```

#### Running the program

To run the program in-place, you can use the `--run` parameter of the compiler, like this:

```bash
gelix --run helloworld.gel
```

Alternatively, you can compile it to an executable by specifying the output location with `-o`:

```bash
gelix -o helloworld helloworld.gel

# Run it:
./helloworld
```

If everything went correctly, you should now see `Hello World!` in your terminal.

Congratulations! You just ran your first gelix program.
