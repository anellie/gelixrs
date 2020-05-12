# Operators

In gelix, there are 2 types of operators:
- Binary operators: `a OPERATOR b`
- Unary operators: `OPERATOR a` OR `a OPERATOR`

Notes:

- "Overload" in the below tables refers to the interface in `std/ops` 
that allows implementing the operator on custom types
- `num` refers to booleans and all integer and floating point types
- `snum` refers to signed integers
- `float` refers to all float types

## Binary operators

Name | S | Types | Example | Overload
--- | --- | --- | --- | ---
Add | + | `num` | `2 + 4 == 6` | `Add<O, T>`
Subtract | - | `num` | `4 - 2 == 2` | `Add<O, T>`
Multiply | * | `num` | `2 * 4 == 8` | `Mul<O, T>`
Divide | / | `num` | `2 + 4 == 6` | `Div<O, T>`
Equality | == | `num` | `(2 == 2) == true` | `Equal<O>`
Less | < | `num` | `2 < 4 == true` | ---
Less Equal | <= | `num` | `2 <= 2 == true` | ---
Greater | > | `num` | `2 > 4 == false` | ---
Greater Equal | >= | `num` | `2 <= 2 == true` | ---
Logic And [1] | and | `bool` | `true and false == false` | ---
Logic Or [1] | or | `bool` | `true or false == true` | ---

[1]: Logic operators will always short-circuit. 

## Unary operators

Name | S | Types | Example | Overload
--- | --- | --- | --- | ---
Not | ! | `bool` | `!true == false` | ---
Negate | - | `snum` `float` | `-(4) == -4` | ---
IndexGet | [] | --- | `a[b]` | `IndexGet<I, T>`
IndexSet | [] = | --- | `a[b] = c` | `IndexSet<I, T>`
