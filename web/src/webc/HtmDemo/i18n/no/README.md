# Feilsøking av HTML og JS-kode online

- Gjengi en feilsøkingslinje under koden og gi CodePen og JSFiddle feilsøkingsportaler
- Husk automatisk den valgte feilsøkingsplattformen
- Støtter injisering av tilbakeringinger gjennom `.gen`-attributtet for dynamisk å generere HTML-, CSS- og JS-kode
- Identifiser automatisk underordnede elementer med `language-js` klassenavn og kjør i JS-modus

## Bruk demoen

```html
<c-htm-demo><pre><code class="language-html">
<h1>Demo</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // Returner [html, css, js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## Grensesnittparametere

### Egenskaper og metoder

- `.gen`: Injisert tilbakeringingsfunksjon. **Denne tilbakeringingen må injiseres for at feilsøkingslinjen skal vises. **
  - parameter:
    - `type`: Injisert innholdstype (`HTML = 1`, `JS = 2`).
    - `text`: Kodeteksten hentet fra det underordnede elementet.
  - Returverdi: `[htm, css, js]`