# Depanarea codului HTML și JS online

- Redați o bară de depanare sub cod și furnizați portaluri de depanare CodePen și JSFiddle
- Rețineți automat platforma de depanare selectată
- Acceptă injectarea de apeluri inverse prin atributul `.gen` pentru a genera în mod dinamic cod HTML, CSS și JS
- Identificați automat elementele secundare cu `language-js` numele clasei și rulați în modul JS

## Utilizați demonstrația

```html
<c-htm-demo><pre><code class="language-html">
<h1>Demo</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // Returnează [html, css, js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## Parametrii interfeței

### Proprietăți și metode

- `.gen`: funcția de apel invers injectată. **Acest apel invers trebuie injectat pentru ca bara de depanare să fie afișată. **
  - parametru:
    - `type`: tip de conținut injectat (`HTML = 1`, `JS = 2`).
    - `text`: textul codului extras din elementul secundar.
  - Valoarea returnată: `[htm, css, js]`