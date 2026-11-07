# Spletno odpravljanje napak v kodi HTML in JS

- Upodobite vrstico za odpravljanje napak pod kodo in zagotovite portala za odpravljanje napak CodePen in JSFiddle
- Samodejno si zapomni izbrano platformo za odpravljanje napak
- Podpira vstavljanje povratnih klicev prek atributa `.gen` za dinamično ustvarjanje kode HTML, CSS in JS
- Samodejno identificira podrejene elemente z imenom razreda `language-js` in jih izvaja v načinu JS

## Uporabite demo

```html
<c-htm-demo><pre><code class="language-html">
<h1>Demo</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // Vrni [html, css, js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## Parametri vmesnika

### Lastnosti in metode

- `.gen`: Vstavljena funkcija povratnega klica. **Ta povratni klic mora biti vstavljen, da se prikaže vrstica za odpravljanje napak. **
  - parameter:
    - `type`: vrsta vbrizgane vsebine (`HTML = 1`, `JS = 2`).
    - `text`: Besedilo kode, ekstrahirano iz podrejenega elementa.
  - Vrnjena vrednost: `[htm, css, js]`