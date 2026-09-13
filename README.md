# mdbook-nvim-treesitter

An [mdBook] preprocessor that highlights code with [tree-sitter], using the
queries [nvim-treesitter] ships and the grammar revisions it pins.

[mdBook]: https://rust-lang.github.io/mdBook/
[tree-sitter]: https://tree-sitter.github.io/tree-sitter/
[nvim-treesitter]: https://github.com/nvim-treesitter/nvim-treesitter

Unlike other mdbook+treesitter solutions, 
the preprocessor downloads `tree-sitter-cli` and `nvim-treesitter` automatically
to a local cache directory, nothing needs to be manually installed. However,
`tree-sitter-cli` depends on a C compiler on the system (looking at the same places
as the `cc` crate).

The nvim-treesitter project also maintains higher-quality queries
than the default ones from upstream grammar repos.

Quality: Slop - other than code organized in layers by me and the meta build script, everything else is generated.

## Quick start

Install the preprocessor either from pre-built binary or from source:
```console
# Install from GitHub release with cargo-binstall
$ cargo binstall mdbook-nvim-treesitter
# .. Or Install from source
$ cargo install mdbook-nvim-treesitter

# Generate the default stylesheet
$ mdbook-nvim-treesitter css > theme/tree-sitter.css
```

Add configuration to `book.toml`

```toml
# book.toml
[preprocessor.nvim-treesitter]

[output.html]
additional-css = ["theme/tree-sitter.css"]
```

Build the book
```console
$ mdbook build
```

The first build clones and compiles the grammars it needs, which takes a
minute or two; later builds reuse the cache and cost nothing. By default,
the system-wide cache location is `~/.cache/mdbook-nvim-treesitter`, and
can be changed with the `MDBOOK_NVIM_TREESITTER_HOME` environment variable.

To make it easy to check/inspect the parsers and queries a book depens on,
the parsers and queries are copied to a local cache directory next to the book,
which is `.cache` by default.

## Configuration

```toml
[preprocessor.nvim-treesitter]
# Highlight only these languages. Empty means "every language
# nvim-treesitter supports" i.e. auto detection
include = []

# Never highlight these, leaving them to mdBook. Useful for Rust if you
# want the playground and hidden-line handling.
exclude = []

# Where this book keeps its parsers and queries, relative to book.toml.
cache_dir = ".cache"
```

Both lists accept anything a code fence accepts, so `exclude = ["sh"]` excludes
bash, and `include = ["c++"]` includes C++.

`include` and `exclude` only apply to *code blocks*. A language that arrives through
an injection -- the JavaScript inside an HTML block, say -- is always
highlighted, since leaving it out would render the surrounding block half
finished.

## Writing code

### Fenced blocks

Anywhere a fence can go: at the top level, in a list item, in a block quote.

````markdown
```rust
fn main() {}
```
````

Annotations after the language are ignored, so `rust,no_run` still highlights
as Rust.

### Inline code

Put the language before an inner backtick, inside a longer fence:

```markdown
the ```rust`let x = 1;``` declaration
```

Ordinary inline code is untouched.

### Opting one block out

Tag it `no-treesitter`. mdBook then handles it as it normally would, which is
what you want for a Rust playground example whose hidden `#` lines mdBook's
JavaScript strips:

````markdown
```rust,no-treesitter
# fn hidden() {}
fn main() {}
```
````

### Overriding a highlight

Sometimes the output is still undesirable without semantic tokens for a language
server. For example, in C, macros and functions look alike. If this matters to you,
you can use an inline comment to manually override the highlight of the next highlighted
token.

```c
/* @tree-sitter:function.macro */ASSERT(x > 0);
```

The comment is removed before the code is parsed -- so it never reaches the
grammar, and the syntax works even in a language where `/* */` means nothing --
and the group it names is applied to the code right after it. `<!-- ... -->` is
accepted as well, for HTML-like languages.

## Styling

Highlights become CSS classes, not inline colours. A dotted capture emits one
class per prefix, so `@keyword.function` becomes:

```html
<span class="ts-keyword ts-keyword-function">fn</span>
```

A stylesheet can then colour every keyword through `ts-keyword`, or just this
kind through `ts-keyword-function`. Because the general class is always present
too, a group with no rule of its own inherits its parent's colour -- the same
fallback Neovim applies to highlight groups.

`mdbook-nvim-treesitter css` emits the default CSS stylesheet, which is the VSCode
dark theme. See [default.css](./packages/cli/src/renderer/default.css)

Elements carry a few structural classes as well:

| Class | On |
| ----- | -- |
| `tree-sitter-block` | the `<pre>` around a fenced block |
| `tree-sitter-code` | the `<code>` of a fenced block |
| `tree-sitter-inline` | the `<code>` of an inline span |
| `tree-sitter-language-<id>` | every `<code>`, for per-language rules |

`no-highlight language-none` is also set on every `<code>` that is touched by tree-sitter.
This is because mdbook runs highlight.js on every code block (even with `no-highlight`),
and that messes up the tree-sitter highlights. It might be a bug in mdbook, but setting
`language-none` is a workaround. Use `tree-sitter-language-<id>` to apply per-language styles.

## Known limitations

- **Rust playground.** Because `language-none` is what keeps highlight.js away,
  mdBook's JavaScript no longer recognises a highlighted block as Rust, so
  hidden `#` lines and the playground buttons do not apply to it. Use
  `no-treesitter` on those blocks, or `exclude = ["rust"]`.
