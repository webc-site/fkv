# Felsökning av HTML och JS-kod online

- Gör en felsökningsstapel under koden och tillhandahåll CodePen och JSFiddle felsökningsportaler
- Kom ihåg automatiskt den valda felsökningsplattformen
- Stöder injicering av återuppringningar genom attributet `.gen` för att dynamiskt generera HTML, CSS och JS-kod
- Identifiera underordnade element automatiskt med `language-js` klassnamn och kör i JS-läge

## Använd demon

```html
<c-htm-demo><pre><code class="language-html">
<h1>Demo</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // Returnera [html, css, js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## Gränssnittsparametrar

### Egenskaper och metoder

- `.gen`: Injicerad återuppringningsfunktion. **Denna återuppringning måste injiceras för att felsökningsfältet ska visas. **
  - parameter:
    - `type`: Injicerad innehållstyp (`HTML = 1`, `JS = 2`).
    - `text`: Kodtexten extraherad från det underordnade elementet.
  - Returvärde: `[htm, css, js]`