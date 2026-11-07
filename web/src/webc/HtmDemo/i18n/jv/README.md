# Debugging HTML lan kode JS online

- Gawe bar debugging ing ngisor kode lan wenehake portal debugging CodePen lan JSFiddle
- Elingi platform debugging sing dipilih kanthi otomatis
- Ndhukung nyuntikake callback liwat atribut `.gen` kanggo ngasilake kode HTML, CSS lan JS kanthi dinamis
- Ngenali kanthi otomatis unsur anak nganggo jeneng kelas `language-js` lan mbukak ing mode JS

## Gunakake demo

```html
<c-htm-demo><pre><code class="language-html">
<h1>Demo</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // Bali [html, css, js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## Paramèter antarmuka

### Properties lan cara

- `.gen`: Fungsi panggilan balik sing disuntikake. **Panggilan balik iki kudu disuntikake supaya bar debug ditampilake. **
  - parameter:
    - `type`: Jinis konten sing disuntik (`HTML = 1`, `JS = 2`).
    - `text`: Teks kode sing diekstrak saka unsur anak.
  - Nilai bali: `[htm, css, js]`