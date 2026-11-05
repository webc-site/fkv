# Tab yang dapat diganti

- Status label yang dipilih terintegrasi secara mulus dengan area konten di bawahnya.
- Spesifikasi struktural: Elemen turunan ditetapkan untuk menggunakan `nav > a` sebagai navigasi label, dan elemen turunan langsung `b` digunakan sebagai area konten.
- Kaitkan tag `a[value]` secara otomatis dengan node konten `b[slot]`.
- Mendukung secara otomatis mempertahankan halaman aktif saat ini ke `localStorage` melalui atribut `key`.
- Mendukung penentuan halaman tab yang diaktifkan melalui atribut `value` (jika tidak ada cache dan `value`, tag dengan `class="A"` atau tag pertama akan dipilih secara default).
- Implementasi DOM Ringan yang ringan, tidak ada halangan Shadow DOM, kebebasan tingkat tinggi dalam penyesuaian gaya.

## Gunakan demonya

```html
<c-tab key="my-tab-key" value="banana">
  <nav>
    <a value="apple">apel</a>
    <a value="banana">pisang</a>
    <a value="orange">jeruk keprok</a>
  </nav>
  <b slot="apple">Apel adalah buah yang bergizi</b>
  <b slot="banana">Pisang adalah buah tropis</b>
  <b slot="orange">Jeruk kaya akan vitamin C</b>
</c-tab>
```

## milik

- `key`: nama kunci yang digunakan untuk persistensi Penyimpanan lokal
- `value`: nilai yang dipilih saat ini

## Acara khusus

- `change`: Dipicu ketika tab dialihkan, `e.value` adalah nilai `value` dari tag yang saat ini diaktifkan.