# HTML- und JS-Code online debuggen

- Rendern Sie eine Debugging-Leiste unter dem Code und stellen Sie CodePen- und JSFiddle-Debugging-Portale bereit
- Merken Sie sich automatisch die ausgewählte Debugging-Plattform
- Unterstützt das Einfügen von Rückrufen über das Attribut `.gen`, um HTML-, CSS- und JS-Code dynamisch zu generieren
- Identifizieren Sie untergeordnete Elemente automatisch mit dem Klassennamen `language-js` und führen Sie sie im JS-Modus aus

## Nutzen Sie die Demo

```html
<c-htm-demo><pre><code class="language-html">
<h1>Demo</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // Rückgabe [html, css, js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## Schnittstellenparameter

### Eigenschaften und Methoden

- `.gen`: Eingefügte Rückruffunktion. **Dieser Rückruf muss eingefügt werden, damit die Debug-Leiste angezeigt wird. **
  - Parameter:
    - `type`: Eingefügter Inhaltstyp (`HTML = 1`, `JS = 2`).
    - `text`: Der aus dem untergeordneten Element extrahierte Codetext.
  - Rückgabewert: `[htm, css, js]`