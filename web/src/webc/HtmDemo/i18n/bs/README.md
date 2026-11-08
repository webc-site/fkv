# Otklanjanje grešaka u HTML i JS kodu na mreži

- Renderirajte traku za otklanjanje grešaka ispod koda i obezbedite portale za otklanjanje grešaka CodePen i JSFiddle
- Automatski zapamtite odabranu platformu za otklanjanje grešaka
- Podržava ubacivanje povratnih poziva putem `.gen` atributa za dinamičko generiranje HTML, CSS i JS koda
- Automatski identificirajte podređene elemente sa `language-js` imenom klase i pokrenite u JS načinu

## Koristite demo

```html
<c-htm-demo><pre><code class="language-html">
<h1>Demo</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // Povratak [html, css, js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## Parametri interfejsa

### Svojstva i metode

- `.gen`: Injektirana funkcija povratnog poziva. **Ovaj povratni poziv mora biti ubačen da bi se prikazala traka za otklanjanje grešaka. **
  - parametar:
    - `type`: Tip ubačenog sadržaja (`HTML = 1`, `JS = 2`).
    - `text`: Tekst koda ekstrahovan iz podređenog elementa.
  - Povratna vrijednost: `[htm, css, js]`