# Menyahpepijat kod HTML dan JS dalam talian

- Berikan bar nyahpepijat di bawah kod dan sediakan portal penyahpepijatan CodePen dan JSFiddle
- Ingat secara automatik platform penyahpepijatan yang dipilih
- Menyokong menyuntik panggilan balik melalui atribut `.gen` untuk menjana kod HTML, CSS dan JS secara dinamik
- Mengenal pasti elemen anak secara automatik dengan nama kelas `language-js` dan jalankan dalam mod JS

## Gunakan demo

```html
<c-htm-demo><pre><code class="language-html">
<h1>Demo</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // Kembalikan [html, css, js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## Parameter antara muka

### Sifat dan kaedah

- `.gen`: Fungsi panggil balik yang disuntik. **Panggil balik ini mesti disuntik untuk bar nyahpepijat dipaparkan. **
  - parameter:
    - `type`: Jenis kandungan yang disuntik (`HTML = 1`, `JS = 2`).
    - `text`: Teks kod yang diekstrak daripada elemen anak.
  - Nilai pulangan: `[htm, css, js]`