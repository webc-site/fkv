# Komponen Navigasi Direktori

- **Laci responsif**: Tampilake sidebar ing layar amba, kanthi otomatis ambruk ing layar sempit lan kerjo bareng karo `c-side` laci lan tombol pemicu judhul
- ** Driver multi-basa **: Ndhukung kamus obyek utawa fungsi multi-basa, kanthi otomatis nanggapi ngoper basa
- **Subdirektori otomatis**: Parsing judhul `h1`~`h6` kanthi otomatis ing wadhah target kanggo ngasilake direktori sekunder lan ngawasi sorotan gulung tampilan Hash.
- **Asynchronous loading**: Klik ing item direktori kanggo otomatis entuk teks Markdown kanthi ora sinkron lan nyuntikake menyang area konten, nampilake `Ing` animasi loading

## Gunakake demo

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
  "use": "nggunakake komponen"
});
</script>
```

## Paramèter antarmuka

### Properties lan cara

- `toc.prefix`: Entuk utawa nyetel string ater-ater routing (kayata `"doc"`, garis miring minangka opsional), tombol `README` bakal kanthi otomatis dipetakan menyang jalur kosong (yaiku ater-ater dhewe)
- `toc.li`: Entuk utawa setel direktori fungsi generasi multi-basa `(lang) => ({ key: title })`
- `toc.mdUrl`: Entuk utawa atur fungsi nggawe URL Markdown `(key, lang) => url`
- `toc.target`: Entuk utawa setel simpul rendering target sing gegandhengan (kayata `c-md`). Yen ora ditemtokake, bakal kanthi otomatis nemokake `c-md` / `main` / `article`
- `toc.load(key)`: Micu kanthi manual ngemot dokumen lan ekspansi direktori sing cocog karo kunci
- `toc.sync()`: Parsing judhul kanthi manual ing simpul target saiki, nyinkronake direktori sekunder lan sorot status