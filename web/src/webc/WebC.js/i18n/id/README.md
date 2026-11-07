# Dokumentasi dan kode sumber untuk merender komponen atau fungsi alat

- Tata letak responsif dua kolom, dengan dokumen ditampilkan di sebelah kiri dan kode sumber ditampilkan di sebelah kanan
- Dokumentasi dan kode sumber mendukung tombol pintas Edit dan Kode untuk melompat ke alamat sumber.
- Judul dan pendahuluan ditampilkan di bagian atas, mendukung terjemahan otomatis i18n.

## Gunakan demonya

```html
<c-webc-js src="/src/js/dom.js" href="/doc/{code}/js/dom.md"></c-webc-js>
```

## Parameter properti

- `src`: Alamat URL kode sumber.
- `href`: Alamat URL penurunan harga dokumen, mendukung penggunaan `{code}` placeholder untuk beradaptasi dengan bahasa saat ini.