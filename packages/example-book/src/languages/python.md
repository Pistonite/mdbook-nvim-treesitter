# Python

```python
"""Word counting, with a docstring the queries pick out separately."""

from __future__ import annotations

import re
from dataclasses import dataclass, field
from typing import Iterable

WORD = re.compile(r"[A-Za-z']+")


@dataclass(frozen=True)
class Counter:
    """Counts words, ignoring case."""

    counts: dict[str, int] = field(default_factory=dict)

    def add(self, text: str) -> None:
        for match in WORD.finditer(text.lower()):
            word = match.group(0)
            self.counts[word] = self.counts.get(word, 0) + 1

    @property
    def total(self) -> int:
        return sum(self.counts.values())


async def gather(sources: Iterable[str]) -> Counter:
    counter = Counter()
    for source in sources:
        counter.add(source)
    return counter


if __name__ == "__main__":
    # f-strings, numeric literals and a walrus.
    counter = Counter()
    counter.add("the quick brown fox")
    if (total := counter.total) > 0:
        print(f"{total=} {0x1F:d} {1_000_000} {3.14e-2!r}")
    else:
        raise SystemExit("nothing counted")
```
