# Ontfout HTML- en JS-kode aanlyn

- Gee 'n ontfoutingsbalk onder die kode en verskaf CodePen en JSFiddle ontfouting portale
- Onthou outomaties die geselekteerde ontfoutingsplatform
- Ondersteun die inspuiting van terugbelopings deur die `.gen`-kenmerk om HTML, CSS en JS-kode dinamies te genereer
- Identifiseer kinderelemente outomaties met `language-js` klasnaam en hardloop in JS-modus

## Gebruik die demo

```html
<c-htm-demo><pre><code class="language-html">
<h1>Demo</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // Keer terug [html, css, js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## Interface parameters

### Eienskappe en metodes

- `.gen`: Ingespuite terugbelfunksie. **Hierdie terugbel moet ingespuit word vir die ontfoutbalk om vertoon te word. **
  - parameter:
    - `type`: Ingevoegde inhoudtipe (`HTML = 1`, `JS = 2`).
    - `text`: Die kodeteks wat uit die kinderelement onttrek is.
  - Retourwaarde: `[htm, css, js]`