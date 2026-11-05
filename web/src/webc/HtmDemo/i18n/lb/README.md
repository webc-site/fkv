# Debugging HTML an JS Code online

- Rendert eng Debugging Bar ënner dem Code a liwwert CodePen a JSFiddle Debugging Portalen
- Erënnert automatesch un déi gewielte Debugging Plattform
- Ënnerstëtzt d'Injektioun vun Callbacks duerch den `.gen` Attribut fir dynamesch HTML, CSS an JS Code ze generéieren
- Identifizéieren automatesch Kand Elementer mat `language-js` Klass Numm a lafen am JS Modus

## Benotzt d'Demo

```html
<c-htm-demo><pre><code class="language-html">
<h1>Demo</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // Return [html, css, js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## Interface Parameteren

### Eegeschaften a Methoden

- `.gen`: Injizéiert Réckrufffunktioun. ** Dëse Callback muss injizéiert ginn fir datt d'Debugbar ugewise gëtt. **
  - parameter:
    - `type`: Injizéiert Inhaltstyp (`HTML = 1`, `JS = 2`).
    - `text`: De Codetext aus dem Kannerelement extrahéiert.
  - Retourwäert: `[htm, css, js]`