# Builtin Types

This page lists all types currently part of the gelix spec.

Name | Type | Literal | Byte Size
--- | --- | --- | ---
`None` | singleton | --- | 0
`bool` | boolean | `true; false` | 1
`i8` | signed int | `14i8` | 1
`i16` | signed int | `14i16` | 2
`i32` | signed int | `14i32` | 4
`i64` | signed int | `14i64` | 8
`isize` | signed int | `14` | [1]
`u8` | unsigned int | `14u8` | 1
`u16` | unsigned int | `14u16` | 2
`u32` | unsigned int | `14u32` | 4
`u64` | unsigned int | `14u64` | 8
`usize` | unsigned int | `14usize` | [1]
`f32` | float | `32.0f32` | 4
`f64` | float | `32.0` | 8

[1]: Size is equal to the pointer size of the target
architecture the compiler was compiled for;
currently `8` on `x86-64` and `4` on `x86`. `usize` and
`isize` are aliased to the respective type.