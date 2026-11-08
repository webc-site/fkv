# Pag-debug ng HTML at JS code online

- Mag-render ng debugging bar sa ibaba ng code at magbigay ng CodePen at JSFiddle debugging portal
- Awtomatikong tandaan ang napiling platform ng pag-debug
- Sinusuportahan ang pag-inject ng mga callback sa pamamagitan ng attribute na `.gen` para dynamic na bumuo ng HTML, CSS at JS code
- Awtomatikong tukuyin ang mga elemento ng bata na may `language-js` pangalan ng klase at tumakbo sa JS mode

## Gamitin ang demo

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

## Mga parameter ng interface

### Mga katangian at pamamaraan

- `.gen`: Injected callback function. **Ang callback na ito ay dapat mai-inject para maipakita ang debug bar. **
  - parameter:
    - `type`: Injected na uri ng content (`HTML = 1`, `JS = 2`).
    - `text`: Ang code text na kinuha mula sa child element.
  - Ibinalik na halaga: `[htm, css, js]`