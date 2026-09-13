# Rust

```rust
use std::collections::HashMap;

/// Counts how often each word appears.
///
/// ```
/// assert_eq!(count("a a b")["a"], 2);
/// ```
pub fn count(text: &str) -> HashMap<&str, usize> {
    let mut counts = HashMap::new();
    for word in text.split_whitespace() {
        *counts.entry(word).or_insert(0) += 1;
    }
    counts
}

#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Circle { radius: f64 },
    Rect(f64, f64),
}

impl Shape {
    const UNIT: f64 = 1.0;

    pub fn area(&self) -> f64 {
        match self {
            Shape::Circle { radius } => std::f64::consts::PI * radius * radius,
            Shape::Rect(w, h) => w * h,
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let shapes = vec![Shape::Circle { radius: 2.0 }, Shape::Rect(3.0, 4.0)];
    let total: f64 = shapes.iter().map(Shape::area).sum();
    println!("total area: {total:.2}, unit {}", Shape::UNIT);

    // A raw string, an escape, and a byte literal.
    let path = r"C:\Users\example";
    let tab = "a\tb\u{1F600}";
    let byte = b'\n';
    assert!(!path.is_empty() && tab.len() > 2 && byte == 10);

    Ok(())
}
```

Macros, lifetimes and generics:

```rust
macro_rules! square {
    ($x:expr) => {
        $x * $x
    };
}

pub trait Store<'a, T: Clone + 'a> {
    type Error;
    fn get(&'a self, key: &str) -> Result<Option<T>, Self::Error>;
}

pub async fn fetch<S>(store: &S) -> u32
where
    S: for<'a> Store<'a, u32, Error = ()>,
{
    square!(store.get("n").ok().flatten().unwrap_or(0))
}
```
