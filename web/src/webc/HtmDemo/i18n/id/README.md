# Men-debug kode HTML dan JS secara online

- Render bilah debug di bawah kode dan sediakan portal debug CodePen dan JSFiddle
- Secara otomatis mengingat platform debugging yang dipilih
- Mendukung memasukkan callback melalui atribut `.gen` untuk menghasilkan kode HTML, CSS, dan JS secara dinamis
- Secara otomatis mengidentifikasi elemen anak dengan nama kelas `language-js` dan dijalankan dalam mode JS

## Gunakan demonya

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

## Parameter antarmuka

### Properti dan metode

- `.gen`: Fungsi panggilan balik yang dimasukkan. **Panggilan balik ini harus dimasukkan agar bilah debug dapat ditampilkan. **
  - parameter:
    - `type`: Jenis konten yang dimasukkan (`HTML = 1`, `JS = 2`).
    - `text`: Teks kode yang diekstraksi dari elemen anak.
  - Nilai pengembalian: `[htm, css, js]`