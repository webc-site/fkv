# Komponen posting blog

- **Pengelompokan tahun otomatis**: Secara otomatis mengklasifikasikan tahun berdasarkan tanggal artikel, dan menyusunnya dalam urutan terbalik berdasarkan tahun
- **Format Tanggal Sederhana**: Secara otomatis menghapus awalan tahun saat menampilkan, dan hanya menampilkan bulan dan hari
- **Ringkasan penurunan harga dan teks**: Pratinjau ringkasan daftar bawaan dan rendering teks lengkap artikel tunggal
- **Judul tiga dimensi gradien**: Integrasikan `H1Txt` untuk menampilkan judul dan tanggal rilis

## Gunakan demonya

```html
<c-blog></c-blog>

<script type="module">
import "webc.site/Blog.js";

const blog = document.querySelector("c-blog");
blog.prefix = "/blog/";
blog.mdUrl = (key, lang) => "/doc/" + lang + "/blog/" + key + ".md";
blog._ = {
  "2026/terbitkan": ["judul", "abstrak", "27-07-2026"]
};

// Beralih untuk memuat isi artikel yang ditentukan (berikan string kosong untuk beralih kembali ke daftar)
blog.load("2026/publish");
</script>
```

## Parameter antarmuka

### Properti dan metode

- `blog.prefix`: Mendapatkan atau menyetel awalan tautan artikel (seperti `"/blog/"`)
- `blog.mdUrl`: Mendapatkan atau menyetel fungsi pembuatan alamat artikel penurunan harga `(key, lang) => url`
- `blog._`: Mendapatkan atau menyetel kamus metadata daftar artikel `{ [key]: [title, summary, date] }`
- `blog.load(key)`: Beralih untuk memuat konten artikel dari kunci yang ditentukan, berikan string kosong `""` untuk beralih kembali ke daftar blog