# Komponen Navigasi Direktori

- **Laci responsif**: Menampilkan sidebar di layar lebar, otomatis diciutkan di layar sempit dan bekerja sama dengan `c-side` laci dan tombol pemicu judul
- **Driver multi-bahasa**: Mendukung kamus objek atau fungsi multi-bahasa, secara otomatis merespons peralihan bahasa
- **Subdirektori otomatis**: Secara otomatis mengurai judul `h1`~`h6` dalam penampung target untuk menghasilkan direktori sekunder dan memantau sorotan gulir area pandang Hash.
- **Pemuatan asinkron**: Klik pada item direktori untuk secara otomatis mendapatkan teks penurunan harga secara asinkron dan memasukkannya ke dalam area konten, menampilkan `Ing` animasi pemuatan

## Gunakan demonya

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
  "gunakan": "gunakan komponen"
});
</script>
```

## Parameter antarmuka

### Properti dan metode

- `toc.prefix`: Dapatkan atau setel string awalan perutean (seperti `"doc"`, garis miring di akhir bersifat opsional), kunci `README` akan secara otomatis dipetakan ke jalur kosong (yaitu awalan itu sendiri)
- `toc.li`: Dapatkan atau setel fungsi pembuatan multi-bahasa direktori `(lang) => ({ key: title })`
- `toc.mdUrl`: Mendapatkan atau menyetel fungsi pembuatan URL penurunan harga `(key, lang) => url`
- `toc.target`: Mendapatkan atau menyetel node rendering target terkait (seperti `c-md`). Jika tidak ditentukan, maka secara otomatis akan menemukan `c-md` / `main` / `article`
- `toc.load(key)`: Secara manual memicu pemuatan dokumen dan perluasan direktori yang sesuai dengan kunci
- `toc.sync()`: Parsing ulang judul secara manual di node target saat ini, sinkronkan direktori sekunder dan sorot status