# Introduction

> [!NOTE]
> Please see the [GitHub README](https://github.com/Pistonite/mdbook-nvim-treesitter) for steps
> to install and configure the preprocessor. This book covers usage and examples.

This book exercises `mdbook-nvim-treesitter`. Every code block in it is
highlighted by tree-sitter, using the queries [nvim-treesitter] ships, with the
parsers built from the grammar revisions that project pins.

[nvim-treesitter]: https://github.com/nvim-treesitter/nvim-treesitter

Highlights become CSS classes rather than inline colours. A `@keyword.function`
capture is rendered as:

```html
<span class="ts-keyword ts-keyword-function">fn</span>
```

so a stylesheet can target `ts-keyword` for every keyword, or
`ts-keyword-function` for just this one. Run `mdbook-nvim-treesitter css` to get
a starting theme.

## Opting out

A single block can be left to mdBook with a `no-treesitter` tag, which is what
you want for a Rust playground example whose hidden lines mdBook strips:

```rust,no-treesitter
# fn hidden() {}
fn main() {
    println!("this block is highlighted by mdBook, not tree-sitter");
}
```


## Overriding a highlight
In some languages, the exact semantic token cannot be determined from the syntax alone,
an example is: function-like macros and functions in C have the same syntax.

`mdbook-nvim-treesitter` allows an inline comment to override the next highlight
if this is important to you

```c,no-treesitter
void check(int x) {
    /* @tree-sitter:function.macro */ASSERT(x > 0);
    report(x);
}
```

Outputs:

```c
void check(int x) {
    /* @tree-sitter:function.macro */ASSERT(x > 0);
    report(x);
}
```

Note the default theme colors them the same way, but you can inspect the class list
on the span and verify it is `ts-function-macro` instead of `ts-function-call`,
compared to the one below which does not have a comment

```c
void check(int x) {
    ASSERT(x > 0);
    report(x);
}
```
