# HTML- ja JS-koodi silumine võrgus

- Renderdage koodi alla silumisriba ja andke CodePen ja JSFiddle silumisportaalid
- Jäta valitud silumisplatvorm automaatselt meelde
- Toetab tagasihelistuste sisestamist atribuudi `.gen` kaudu HTML-, CSS- ja JS-koodi dünaamiliseks genereerimiseks
- Tuvastage alamelemendid automaatselt `language-js` klassi nimega ja käivitage JS-režiimis

## Kasutage demo

```html
<c-htm-demo><pre><code class="language-html">
<h1>Demo</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // Tagasta [html, css, js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## Liidese parameetrid

### Omadused ja meetodid

- `.gen`: sisestatud tagasihelistamise funktsioon. **Silumisriba kuvamiseks tuleb see tagasihelistus sisestada. **
  - parameeter:
    - `type`: sisestatud sisutüüp (`HTML = 1`, `JS = 2`).
    - `text`: alamelemendist eraldatud kooditekst.
  - Tagastusväärtus: `[htm, css, js]`