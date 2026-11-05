# Villuleit HTML og JS kóða á netinu

- Gerðu villuleitarstiku fyrir neðan kóðann og útvegaðu CodePen og JSFiddle villuleitargáttir
- Mundu sjálfkrafa valinn villuleitarvettvang
- Styður innspýting svarhringinga í gegnum `.gen` eigindina til að búa til HTML, CSS og JS kóða á virkan hátt
- Auðkenndu sjálfkrafa undireiningar með `language-js` flokksheiti og keyrðu í JS ham

## Notaðu kynninguna

```html
<c-htm-demo><pre><code class="language-html">
<h1>Demo</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // Skila [html, css, js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## Viðmótsbreytur

### Eiginleikar og aðferðir

- `.gen`: Inndælt svarhringingaraðgerð. **Þessi svarhringing verður að vera sprautuð inn til að villuleitarstikan birtist. **
  - færibreyta:
    - `type`: Inndælt efnistegund (`HTML = 1`, `JS = 2`).
    - `text`: Kóðatextinn dreginn út úr undireiningunni.
  - Skilagildi: `[htm, css, js]`