# Komponen Navigasi Direktori

- **Laci responsif**: Paparkan bar sisi dalam skrin lebar, runtuh secara automatik dalam skrin sempit dan bekerjasama dengan `c-side` laci dan butang pencetus tajuk
- **Pemandu berbilang bahasa**: Menyokong kamus objek atau fungsi berbilang bahasa, bertindak balas secara automatik kepada penukaran bahasa
- **Subdirektori automatik**: Menghuraikan tajuk `h1`~`h6` secara automatik dalam bekas sasaran untuk menjana direktori kedua dan memantau sorotan tatal Hash viewport.
- **Pemuatan tak segerak**: Klik pada item direktori untuk mendapatkan teks Markdown secara automatik secara tidak segerak dan menyuntiknya ke dalam kawasan kandungan, memaparkan `Ing` memuatkan animasi

## Gunakan demo

```html
<c-toc></c-toc>
<c-md></c-md>

<script type="module">
import "webc.site/Toc.js";

const toc = document.querySelector("c-toc");
toc.target = document.querySelector("c-md");
toc.prefix = "doc";
toc.mdUrl = (key, lang) => "/doc/" + lang + "/" + key + ".md";
toc.li = (lang) => ({
  "README": "WebC.site",
  "use": "gunakan komponen"
});
</script>
```

## Parameter antara muka

### Sifat dan kaedah

- `toc.prefix`: Dapatkan atau tetapkan rentetan awalan penghalaan (seperti `"doc"`, garis miring adalah pilihan), kunci `README` akan dipetakan secara automatik ke laluan kosong (iaitu awalan itu sendiri)
- `toc.li`: Dapatkan atau tetapkan fungsi penjanaan berbilang bahasa direktori `(lang) => ({ key: title })`
- `toc.mdUrl`: Dapatkan atau tetapkan fungsi penjanaan URL Markdown `(key, lang) => url`
- `toc.target`: Dapatkan atau tetapkan nod pemaparan sasaran yang berkaitan (seperti `c-md`). Jika tidak dinyatakan, ia akan mencari `c-md` / `main` / `article` secara automatik
- `toc.load(key)`: Cetuskan pemuatan dokumen dan pengembangan direktori secara manual yang sepadan dengan kunci
- `toc.sync()`: Menghuraikan semula tajuk secara manual dalam nod sasaran semasa, menyegerakkan direktori kedua dan menyerlahkan status