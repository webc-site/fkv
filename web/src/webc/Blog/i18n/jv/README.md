# Komponen kirim blog

- **Pengelompokan taun otomatis**: Golongake taun kanthi otomatis miturut tanggal artikel, lan atur miturut urutan taun
- **Format Tanggal Sederhana**: Mbusak awalan taun kanthi otomatis nalika ditampilake, lan mung nampilake sasi lan dina
- **Ringkesan lan teks markdown**: Pratinjau ringkesan dhaptar terintegrasi lan rendering teks lengkap artikel siji
- **Judul telung dimensi gradien**: Integrasi `H1Txt` kanggo nampilake judhul lan tanggal rilis

## Gunakake demo

```html
<c-blog></c-blog>

<script type="module">
import "webc.site/Blog.js";

const blog = document.querySelector("c-blog");
blog.prefix = "/blog/";
blog.mdUrl = (key, lang) => "/doc/" + lang + "/blog/" + key + ".md";
blog._ = {
  "2026/publish": ["judul", "abstrak", "2026-07-27"]
};

// Ngalih kanggo mbukak awak artikel sing ditemtokake (liwat string kosong kanggo bali menyang dhaptar)
blog.load("2026/publish");
</script>
```

## Paramèter antarmuka

### Properties lan cara

- `blog.prefix`: Entuk utawa setel awalan pranala artikel (kayata `"/blog/"`)
- `blog.mdUrl`: Entuk utawa setel fungsi nggawe alamat artikel Markdown `(key, lang) => url`
- `blog._`: Entuk utawa atur kamus metadata dhaptar artikel `{ [key]: [title, summary, date] }`
- `blog.load(key)`: Ngalih kanggo mbukak isi artikel saka kunci sing ditemtokake, lewati string kosong `""` kanggo bali menyang dhaptar blog