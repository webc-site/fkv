# Dadfygio cod HTML a JS ar-lein

- Rendro bar dadfygio o dan y cod a darparu pyrth dadfygio CodePen a JSFiddle
- Cofiwch yn awtomatig y platfform dadfygio a ddewiswyd
- Yn cefnogi chwistrellu galwadau'n ôl trwy'r briodwedd `.gen` i gynhyrchu cod HTML, CSS a JS yn ddeinamig
- Adnabod elfennau plentyn yn awtomatig ag `language-js` enw dosbarth a rhedeg yn y modd JS

## Defnyddiwch y demo

```html
<c-htm-demo><pre><code class="language-html">
<h1>Demo</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // Dychwelyd [html, css, js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## Paramedrau rhyngwyneb

### Priodweddau a dulliau

- `.gen`: Swyddogaeth galw'n ôl wedi'i chwistrellu. **Rhaid chwistrellu'r alwad hon yn ôl er mwyn i'r bar dadfygio gael ei ddangos. **
  - paramedr:
    - `type`: Math o gynnwys wedi'i chwistrellu (`HTML = 1`, `JS = 2`).
    - `text`: Testun y cod a dynnwyd o'r elfen plentyn.
  - Gwerth dychwelyd: `[htm, css, js]`