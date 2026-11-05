# Online debuggen van HTML- en JS-code

- Geef een foutopsporingsbalk weer onder de code en zorg voor foutopsporingsportalen van CodePen en JSFiddle
- Onthoud automatisch het geselecteerde foutopsporingsplatform
- Ondersteunt het injecteren van callbacks via het `.gen` attribuut om dynamisch HTML-, CSS- en JS-code te genereren
- Identificeer automatisch onderliggende elementen met `language-js` klassenaam en voer deze uit in JS-modus

## Gebruik de demo

```html
<c-htm-demo><pre><code class="language-html">
<h1>Demo</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // Retourneer [html, css, js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## Interfaceparameters

### Eigenschappen en methoden

- `.gen`: Geïnjecteerde callback-functie. **Deze callback moet worden geïnjecteerd om de foutopsporingsbalk weer te geven. **
  - parameter:
    - `type`: geïnjecteerd inhoudstype (`HTML = 1`, `JS = 2`).
    - `text`: De codetekst die uit het onderliggende element wordt geëxtraheerd.
  - Retourwaarde: `[htm, css, js]`