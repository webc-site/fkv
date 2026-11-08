# HTML un JS koda atkļūdošana tiešsaistē

- Atveidojiet atkļūdošanas joslu zem koda un nodrošiniet CodePen un JSFiddle atkļūdošanas portālus
- Automātiski atcerēties atlasīto atkļūdošanas platformu
- Atbalsta atzvanīšanas ievadīšanu, izmantojot atribūtu `.gen`, lai dinamiski ģenerētu HTML, CSS un JS kodu
- Automātiski identificēt bērnu elementus ar `language-js` klases nosaukumu un palaist JS režīmā

## Izmantojiet demonstrāciju

```html
<c-htm-demo><pre><code class="language-html">
<h1>Demonstrācija</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // Atgriezt [html, css, js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## Interfeisa parametri

### Īpašības un metodes

- `.gen`: ievadīta atzvanīšanas funkcija. **Šis atzvans ir jāievada, lai tiktu parādīta atkļūdošanas josla. **
  - parametrs:
    - `type`: ievadītā satura veids (`HTML = 1`, `JS = 2`).
    - `text`: koda teksts, kas izvilkts no pakārtotā elementa.
  - Atgriešanas vērtība: `[htm, css, js]`