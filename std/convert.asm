/// A function that takes a field element or integer and converts
/// it to a (non-negative) integer.
/// The actual implementation is replaced by a built-in function.
let int = [];

/// A function that takes a field element or integer and converts
/// it to a field element.
/// Panics if the input is negative or larger or equal to the field modulus.
/// The actual implementation is replaced by a built-in function.
let fe = [];

/// Converts a function `int -> int` to a column, i.e. converts its
/// return type to field element.
let to_col: (int -> int) -> col = |f| |i| fe(f(i));