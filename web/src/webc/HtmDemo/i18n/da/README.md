# Fejlretning af HTML og JS-kode online

- Gengiv en fejlretningslinje under koden og giv CodePen og JSFiddle fejlfindingsportaler
- Husk automatisk den valgte fejlretningsplatform
- Understøtter indsprøjtning af tilbagekald via attributten `.gen` for dynamisk at generere HTML-, CSS- og JS-kode
- Identificer automatisk underordnede elementer med `language-js` klassenavn og kør i JS-tilstand

## Brug demoen

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

## Interface parametre

### Egenskaber og metoder

- `.gen`: Injiceret tilbagekaldsfunktion. **Dette tilbagekald skal injiceres for at fejlfindingslinjen kan vises. **
  - parameter:
    - `type`: Injiceret indholdstype (`HTML = 1`, `JS = 2`).
    - `text`: Kodeteksten udtrukket fra det underordnede element.
  - Returværdi: `[htm, css, js]`