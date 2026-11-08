# HTML- ja JS-koodin virheenkorjaus verkossa

- Piirrä virheenkorjauspalkki koodin alle ja anna CodePen- ja JSFiddle-virheenkorjausportaalit
- Muista automaattisesti valittu virheenkorjausalusta
- Tukee takaisinsoittojen lisäämistä `.gen`-attribuutin kautta HTML-, CSS- ja JS-koodin dynaamiseen luomiseen
- Tunnista alielementit automaattisesti `language-js`-luokan nimellä ja suorita JS-tilassa

## Käytä demoa

```html
<c-htm-demo><pre><code class="language-html">
<h1>Demo</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // Palauta [html, css, js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## Käyttöliittymän parametrit

### Ominaisuudet ja menetelmät

- `.gen`: Lisätty takaisinsoittotoiminto. **Tämä takaisinsoitto on lisättävä, jotta virheenkorjauspalkki tulee näkyviin. **
  - parametri:
    - `type`: Lisätty sisältötyyppi (`HTML = 1`, `JS = 2`).
    - `text`: Alielementistä poimittu kooditeksti.
  - Palautusarvo: `[htm, css, js]`