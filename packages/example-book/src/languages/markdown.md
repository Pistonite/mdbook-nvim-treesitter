# Markdown

A markdown block injects whatever its own fenced blocks are tagged with, so the
sample below needs the Rust and Python parsers on top of markdown's own two
grammars (`markdown` for block structure, `markdown_inline` for the rest).

````markdown
# A document

Some **bold** text, some *emphasis*, a [link](https://example.com) and
`inline code`.

- a list item
- another, with a nested fence:

  ```rust
  fn nested() -> u8 { 1 }
  ```

> A quote with a fence in it:
>
> ```python
> def nested():
>     return 1
> ```

| Column | Column |
| ------ | ------ |
| a      | b      |
````
