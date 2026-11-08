# HTML ir JS kodų derinimas internete

- Pateikite derinimo juostą po kodu ir pateikite CodePen ir JSFiddle derinimo portalus
- Automatiškai prisiminti pasirinktą derinimo platformą
- Palaiko atgalinių skambučių įvedimą naudojant `.gen` atributą, kad būtų dinamiškai generuojamas HTML, CSS ir JS kodas
- Automatiškai identifikuokite antrinius elementus naudodami `language-js` klasės pavadinimą ir paleiskite JS režimu

## Naudokite demonstracinę versiją

```html
<c-htm-demo><pre><code class="language-html">
<h1>Demo</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // Grąžinti [html, css, js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## Sąsajos parametrai

### Savybės ir metodai

- `.gen`: įvesta atgalinio skambinimo funkcija. **Šis atgalinis skambutis turi būti įvestas, kad būtų rodoma derinimo juosta. **
  - parametras:
    - `type`: įterpto turinio tipas (`HTML = 1`, `JS = 2`).
    - `text`: kodo tekstas, išskirtas iš antrinio elemento.
  - Grąžinama vertė: `[htm, css, js]`