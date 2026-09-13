# The web stack

The interesting part here is injection: an HTML document's `<script>` and
`<style>` contents are separate languages, and the queries say so. Highlighting
this page needed the JavaScript and CSS parsers even though no block is tagged
with either.

## HTML, with JavaScript and CSS inside it

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <title>Injected languages</title>
    <style>
      :root {
        --accent: #4ec9b0;
      }

      .card > h1::after {
        content: "!";
        color: var(--accent);
      }

      @media (max-width: 40rem) {
        .card {
          padding: 0.5rem 1rem;
        }
      }
    </style>
  </head>
  <body>
    <!-- A comment, then a script. -->
    <div class="card" data-id="1">
      <h1>Hello</h1>
    </div>
    <script type="module">
      const card = document.querySelector(".card");
      card?.addEventListener("click", (event) => {
        event.preventDefault();
        console.log(`clicked ${card.dataset.id}`);
      });
    </script>
  </body>
</html>
```

## CSS on its own

```css
@import url("reset.css");

:root {
  --spacing: 8px;
  --font: "Inter", system-ui, sans-serif;
}

.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(12rem, 1fr));
  gap: calc(var(--spacing) * 2);
}

.grid:hover > .cell:not(.disabled)::before {
  content: "→";
  transform: translateX(-100%) rotate(45deg);
  transition: transform 150ms ease-in-out;
}

@supports (backdrop-filter: blur(4px)) {
  .overlay {
    backdrop-filter: blur(4px);
  }
}
```
