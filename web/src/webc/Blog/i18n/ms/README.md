# Komponen catatan blog

- **Penghimpunan tahun automatik**: Klasifikasikan tahun secara automatik mengikut tarikh artikel, dan susunkannya dalam susunan terbalik mengikut tahun
- **Format Tarikh Ringkas**: Alih keluar awalan tahun secara automatik apabila dipaparkan dan hanya memaparkan bulan dan hari
- **Ringkasan dan teks markdown**: Pratonton ringkasan senarai terbina dalam dan pemaparan teks lengkap artikel tunggal
- **Tajuk tiga dimensi kecerunan**: Sepadukan `H1Txt` untuk memaparkan tajuk dan tarikh keluaran

## Gunakan demo

```html
<c-blog></c-blog>

<script type="module">
import "webc.site/Blog.js";

const blog = document.querySelector("c-blog");
blog.prefix = "/blog/";
blog.mdUrl = (key, lang) => "/doc/" + lang + "/blog/" + key + ".md";
blog._ = {
  "2026/publish": ["tajuk", "abstrak", "2026-07-27"]
};

// Tukar untuk memuatkan badan artikel yang ditentukan (lalukan rentetan kosong untuk beralih kembali ke senarai)
blog.load("2026/publish");
</script>
```

## Parameter antara muka

### Sifat dan kaedah

- `blog.prefix`: Dapatkan atau tetapkan awalan pautan artikel (seperti `"/blog/"`)
- `blog.mdUrl`: Dapatkan atau tetapkan fungsi penjanaan alamat artikel Markdown `(key, lang) => url`
- `blog._`: Dapatkan atau tetapkan kamus metadata senarai artikel `{ [key]: [title, summary, date] }`
- `blog.load(key)`: Tukar untuk memuatkan kandungan artikel kunci yang ditentukan, hantar rentetan kosong `""` untuk beralih kembali ke senarai blog