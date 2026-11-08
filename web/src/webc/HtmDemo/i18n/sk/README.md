# Ladenie HTML a JS kódu online

- Vykreslite lištu ladenia pod kódom a poskytnite portály na ladenie CodePen a JSFiddle
- Automaticky si zapamätať vybranú platformu ladenia
- Podporuje vkladanie spätných volaní prostredníctvom atribútu `.gen` na dynamické generovanie kódu HTML, CSS a JS
- Automaticky identifikujte podradené prvky s `language-js` názvom triedy a spustite v režime JS

## Použite demo

```html
<c-htm-demo><pre><code class="language-html">
<h1>Demo</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // Návrat [html, css, js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## Parametre rozhrania

### Vlastnosti a metódy

- `.gen`: Vložená funkcia spätného volania. **Toto spätné volanie musí byť vložené, aby sa zobrazil panel ladenia. **
  - parameter:
    - `type`: Typ vloženého obsahu (`HTML = 1`, `JS = 2`).
    - `text`: Text kódu extrahovaný z podradeného prvku.
  - Návratová hodnota: `[htm, css, js]`