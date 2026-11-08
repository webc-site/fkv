# Ladění HTML a JS kódu online

- Vykreslete lištu ladění pod kódem a poskytněte portály pro ladění CodePen a JSFiddle
- Automaticky si zapamatovat vybranou platformu ladění
- Podporuje vkládání zpětných volání prostřednictvím atributu `.gen` pro dynamické generování kódu HTML, CSS a JS
- Automaticky identifikujte podřízené prvky pomocí názvu třídy `language-js` a spusťte v režimu JS

## Použijte demo

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

## Parametry rozhraní

### Vlastnosti a metody

- `.gen`: Vložená funkce zpětného volání. **Toto zpětné volání musí být vloženo, aby se zobrazil panel ladění. **
  - parametr:
    - `type`: Typ vloženého obsahu (`HTML = 1`, `JS = 2`).
    - `text`: Text kódu extrahovaný z podřízeného prvku.
  - Návratová hodnota: `[htm, css, js]`