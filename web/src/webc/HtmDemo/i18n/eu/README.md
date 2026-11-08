# HTML eta JS kodea sarean araztea

- Errendatu arazketa-barra kodearen azpian eta eman CodePen eta JSFiddle arazketa-atariak
- Gogoratu automatikoki hautatutako arazketa-plataforma
- `.gen` atributuaren bidez deiak injektatzea onartzen du HTML, CSS eta JS kodea dinamikoki sortzeko
- Identifikatu automatikoki elementu haurrak `language-js` klase-izena duten eta exekutatu JS moduan

## Erabili demoa

```html
<c-htm-demo><pre><code class="language-html">
<h1>Demo</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // Itzuli [html, css, js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## Interfazearen parametroak

### Propietateak eta metodoak

- `.gen`: injektatutako deia itzultzeko funtzioa. **Dei hau injektatu behar da arazketa-barra bistaratzeko. **
  - parametroa:
    - `type`: Injektatutako eduki mota (`HTML = 1`, `JS = 2`).
    - `text`: elementu umetik ateratako kode-testua.
  - Itzultzeko balioa: `[htm, css, js]`