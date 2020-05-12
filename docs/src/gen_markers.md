# Generic Markers

Every type in gelix implements a set of markers that can be used as generic bounds.
For more info on generic bounds, see the [generics chapter](generics.md).

`Any` in this table refers to all types.

Name | Implementors | Notes
--- | --- | --- 
Primitive | `bool` `i8` `i16` `i32` `i64` `u8` `u16` `u32` `u64` `f32` `f64`
Number | `bool` `i8` `i16` `i32` `i64` `u8` `u16` `u32` `u64` `f32` `f64`
Integer |`bool` `i8` `i16` `i32` `i64` `u8` `u16` `u32` `u64` | `bool`s can be used exactly like integers.
SignedInt |`i8` `i16` `i32` `i64`
UnsignedInt | `u8` `u16` `u32` `u64`
Float | `f32` `f64`
IsPointer | `*Any` `Class` `ExtClass` `Enum` `EnumCase` | Types represented by a pointer in the LLVM IR type system
IsValue | `^Any` `Primitive` `Interface` | Types represented by a struct or primitive in LLVM IR
Class | Classes | Also includes extern classes.
Interface | Interfaces
ExtClass | Extern classes
Enum | Enums | Does not include enum cases.
EnumCase | Enum cases
