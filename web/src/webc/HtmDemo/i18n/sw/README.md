# Kutatua HTML na msimbo wa JS mtandaoni

- Toa upau wa utatuzi chini ya msimbo na utoe lango la utatuzi la CodePen na JSFiddle
- Kumbuka kiotomatiki jukwaa la utatuzi lililochaguliwa
- Inaauni kuingiza simu kupitia `.gen` sifa ili kuzalisha HTML, CSS na msimbo wa JS kwa nguvu.
- Tambua vipengele vya mtoto kiotomatiki kwa `language-js` jina la darasa na uendeshe katika hali ya JS

## Tumia onyesho

```html
<c-htm-demo><pre><code class="language-html">
<h1>Onyesho</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // Rudi [html, css, js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## Vigezo vya interface

### Mali na mbinu

- `.gen`: Chaguo za kurudisha nyuma zilizoingizwa. **Piga simu hii lazima iingizwe ili upau wa utatuzi uonyeshwe. **
  - kigezo:
    - `type`: Aina ya maudhui yaliyoingizwa (`HTML = 1`, `JS = 2`).
    - `text`: Nakala ya msimbo iliyotolewa kutoka kwa kipengele cha mtoto.
  - Thamani ya kurejesha: `[htm, css, js]`