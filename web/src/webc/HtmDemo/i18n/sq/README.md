# Korrigjimi i kodit HTML dhe JS në internet

- Paraqitni një shirit korrigjimi poshtë kodit dhe siguroni portalet e korrigjimit të CodePen dhe JSFiddle
- Mbani mend automatikisht platformën e zgjedhur të korrigjimit
- Mbështet injektimin e kthimeve të thirrjeve përmes atributit `.gen` për të gjeneruar në mënyrë dinamike kodin HTML, CSS dhe JS
- Identifikoni automatikisht elementët fëmijë me emrin e klasës `language-js` dhe ekzekutoni në modalitetin JS

## Përdorni demonstrimin

```html
<c-htm-demo><pre><code class="language-html">
<h1>Demo</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // Kthehu [html, css, js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## Parametrat e ndërfaqes

### Vetitë dhe metodat

- `.gen`: Funksioni i kthimit të thirrjes së injektuar. **Kjo kthim kthimi duhet të injektohet që të shfaqet shiriti i korrigjimit. **
  - parametri:
    - `type`: Lloji i përmbajtjes së injektuar (`HTML = 1`, `JS = 2`).
    - `text`: Teksti i kodit i nxjerrë nga elementi fëmijë.
  - Vlera e kthimit: `[htm, css, js]`