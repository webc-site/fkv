# Otklanjanje pogrešaka u HTML i JS kodu na mreži

- Renderirajte traku za ispravljanje pogrešaka ispod koda i osigurajte portale za ispravljanje pogrešaka CodePen i JSFiddle
- Automatski zapamti odabranu platformu za otklanjanje pogrešaka
- Podržava ubacivanje povratnih poziva putem atributa `.gen` za dinamičko generiranje HTML, CSS i JS koda
- Automatski identificirajte podređene elemente s `language-js` nazivom klase i pokrenite u JS načinu

## Koristite demo

```html
<c-htm-demo><pre><code class="language-html">
<h1>Demo</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // Vrati [html, css, js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## Parametri sučelja

### Svojstva i metode

- `.gen`: Umetnuta funkcija povratnog poziva. **Ovaj povratni poziv mora biti umetnut da bi se prikazala traka za otklanjanje pogrešaka. **
  - parametar:
    - `type`: Vrsta umetnutog sadržaja (`HTML = 1`, `JS = 2`).
    - `text`: Tekst koda izdvojen iz podređenog elementa.
  - Povratna vrijednost: `[htm, css, js]`