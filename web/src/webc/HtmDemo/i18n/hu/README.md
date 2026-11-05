# HTML és JS kód hibakeresése online

- Jelenítsen meg egy hibakereső sávot a kód alatt, és biztosítson CodePen és JSFiddle hibakereső portált
- Automatikusan megjegyzi a kiválasztott hibakereső platformot
- Támogatja a visszahívások beszúrását a `.gen` attribútumon keresztül a HTML, CSS és JS kód dinamikus generálásához
- Automatikusan azonosítja a gyermekelemeket `language-js` osztálynévvel, és futtassa JS módban

## Használd a demót

```html
<c-htm-demo><pre><code class="language-html">
<h1>Demó</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // Visszatérés [html, css, js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## Interfész paraméterei

### Tulajdonságok és módszerek

- `.gen`: Beszúrt visszahívási funkció. ** Ezt a visszahívást be kell adni a hibakereső sáv megjelenítéséhez. **
  - paraméter:
    - `type`: Beszúrt tartalomtípus (`HTML = 1`, `JS = 2`).
    - `text`: A gyermekelemből kinyert kódszöveg.
  - Visszatérési érték: `[htm, css, js]`