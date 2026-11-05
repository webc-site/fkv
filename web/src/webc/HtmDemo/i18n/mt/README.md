# Debugging HTML u kodiċi JS onlajn

- Agħti bar tad-debugging taħt il-kodiċi u ipprovdi portali tad-debugging CodePen u JSFiddle
- Ftakar awtomatikament il-pjattaforma tad-debugging magħżula
- Jappoġġja l-injezzjoni ta' callbacks permezz tal-attribut `.gen` biex jiġġenera b'mod dinamiku kodiċi HTML, CSS u JS
- Identifika awtomatikament elementi tfal b'`language-js` isem tal-klassi u ħaddem fil-modalità JS

## Uża d-demo

```html
<c-htm-demo><pre><code class="language-html">
<h1>Demo</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // Ritorn [html, css, js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## Parametri tal-interface

### Proprjetajiet u metodi

- `.gen`: Funzjoni ta' callback injettata. **Din is-callback trid tiġi injettata biex tintwera l-bar tad-debug. **
  - parametru:
    - `type`: Tip ta' kontenut injettat (`HTML = 1`, `JS = 2`).
    - `text`: It-test tal-kodiċi estratt mill-element tifel.
  - Valur tar-ritorn: `[htm, css, js]`