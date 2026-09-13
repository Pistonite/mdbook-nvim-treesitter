# Where code can live

Markdown lets code appear in several places, and each needs slightly different
handling to splice highlighted HTML back in without breaking the surrounding
structure.

## At the top level

The simple case: a fence at column 0.

```rust
fn main() {
    let greeting = "hello";
    println!("{greeting}, world");
}
```

## Inside a list

Indented fenced blocks such as the ones inside a list also works:

1. First, define the function:

   ```python
   def area(radius: float) -> float:
       return 3.14159 * radius ** 2
   ```

2. Then call it:

   ```python
   print(area(2.0))
   ```

   - Even nested two levels deep:

     ```c
     int main(void) { return 0; }
     ```

## Inside a block quote

> A quoted example:
>
> ```rust
> #[derive(Debug)]
> struct Point { x: f64, y: f64 }
> ```
>
> Deeply nested block quotes also work
> > ```rust
> > #[derive(Debug)]
> > struct Point { x: f64, y: f64 }
> > ```
>
> ...and the quote continues afterwards.

## Inline

Markdown has no syntax for tagging inline code blocks. We parse each
inline block to inspect a language tag before the first backtick (``` ` ```):
```
```rust`let x = 1;``` # use triple-backtick to quote the backtick, tagging as rust
```

This gives: ```rust`let x = 1;```.

It works for any language, so ```python`lambda x: x + 1``` and
```c`static const int N = 8;``` are highlighted too. Ordinary inline code such as
`--flag` or `book.toml` is left exactly as it was.

## Mixed into a table

| Language | Declaration |
| -------- | ----------- |
| Rust     | ```rust`let x: u8 = 1;``` |
| C        | ```c`uint8_t x = 1;``` |
| Python   | ```python`x: int = 1``` |
