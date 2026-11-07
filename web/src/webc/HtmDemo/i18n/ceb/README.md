# Pag-debug sa HTML ug JS code online

- Paghatag ug debugging bar ubos sa code ug paghatag sa CodePen ug JSFiddle debugging portals
- Awtomatikong hinumdomi ang gipili nga platform sa pag-debug
- Nagsuporta sa pag-inject sa mga callback pinaagi sa `.gen` attribute aron dinamikong makamugna og HTML, CSS ug JS code
- Awtomatikong mailhan ang mga elemento sa bata nga adunay `language-js` ngalan sa klase ug modagan sa JS mode

## Gamita ang demo

```html
<c-htm-demo><pre><code class="language-html">
<h1>Demo</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // Ibalik [html, css, js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## Mga parametro sa interface

### Mga kabtangan ug pamaagi

- `.gen`: Gi-injected callback function. **Kini nga callback kinahanglang ma-injected para mapakita ang debug bar. **
  - parameter:
    - `type`: Gi-inject nga tipo sa sulod (`HTML = 1`, `JS = 2`).
    - `text`: Ang code text nga gikuha gikan sa child element.
  - Ibalik nga bili: `[htm, css, js]`