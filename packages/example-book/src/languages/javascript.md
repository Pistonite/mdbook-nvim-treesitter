# JavaScript and TypeScript

JavaScript's queries inherit from `ecma` and `jsx`; TypeScript's from `ecma`.
Neither of those is a grammar in its own right -- they exist only to be
inherited from.

## JavaScript

```javascript
import { readFile } from "node:fs/promises";

/**
 * Loads a config file.
 * @param {string} path
 * @returns {Promise<Record<string, unknown>>}
 */
export async function loadConfig(path) {
  const text = await readFile(path, "utf8");
  return JSON.parse(text);
}

export class Cache extends Map {
  #hits = 0;

  static from(entries) {
    return new Cache(entries);
  }

  get(key) {
    const value = super.get(key);
    if (value !== undefined) this.#hits += 1;
    return value ?? null;
  }

  get hits() {
    return this.#hits;
  }
}

const NAMES = /^[a-z][\w-]*$/iu;
const tagged = (strings, ...values) => String.raw({ raw: strings }, ...values);

for (const [key, value] of Object.entries({ a: 1, b: 2 })) {
  if (!NAMES.test(key)) continue;
  console.log(tagged`${key} => ${value}`);
}
```

## TypeScript

```typescript
type Result<T, E = Error> = { ok: true; value: T } | { ok: false; error: E };

interface Repository<T> {
  readonly name: string;
  find(id: string): Promise<T | undefined>;
}

export enum Level {
  Debug = "debug",
  Info = "info",
}

export abstract class Base<T extends { id: string }> implements Repository<T> {
  protected constructor(public readonly name: string) {}

  abstract find(id: string): Promise<T | undefined>;

  async findOrFail(id: string): Promise<Result<T>> {
    const found = await this.find(id);
    return found
      ? { ok: true, value: found }
      : { ok: false, error: new Error(`missing ${id}`) };
  }
}

export function isLevel(value: unknown): value is Level {
  return typeof value === "string" && Object.values(Level).includes(value as Level);
}
```

## JSX

`jsx` and `ecma` are query-only languages in nvim-treesitter -- they have no
grammar of their own and exist so that `javascript`, `typescript` and `tsx` can
inherit their queries. They are *also* Neovim filetype aliases for JavaScript,
so a fence tagged with either resolves to JavaScript, which does have a parser
and which pulls those queries in anyway.

```jsx
export function Card({ title, children }) {
  const [open, setOpen] = useState(false);
  return (
    <article className="card" data-open={open}>
      <h2 onClick={() => setOpen(!open)}>{title}</h2>
      {open && <div className="body">{children}</div>}
    </article>
  );
}
```
