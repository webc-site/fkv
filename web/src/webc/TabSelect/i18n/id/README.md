# Halaman tab dengan pilihan drop-down

- Mendukung penyematan kotak pilihan drop-down lebar adaptif (`<c-select>`) di label, yang hanya akan ditampilkan ketika halaman tab saat ini diaktifkan.
- Spesifikasi struktural: Elemen turunan menggunakan `nav > a` sebagai navigasi label, dan elemen turunan langsung `b` berfungsi sebagai area konten.
- Secara otomatis mengaitkan tag `a[value]`, `a` dengan `c-select` (berdasarkan atribut value grup dan nilai opsi yang dipilih) dan node konten `b[slot]`.
- Mendukung penyimpanan otomatis item yang saat ini diaktifkan ke `localStorage` melalui atribut `key`.
- Mendukung penyimpanan otomatis dan pemulihan sub-pilihan historis untuk setiap grup ke `localStorage` menggunakan `key + '/' + tab`.
- Mendukung penentuan tab aktif melalui atribut `value`.

## Gunakan demonya

```html
<c-tab-select key="my-tab-select-key" value="language/js">
  <nav>
    <a value="language">
      <span>bahasa</span>
      <c-select>
        <option value="js">JavaScript</option>
        <option value="rs">Rust</option>
      </c-select>
    </a>
    <a value="os">
      <span>sistem</span>
      <c-select>
        <option value="mac">macOS</option>
        <option value="win">Windows</option>
      </c-select>
    </a>
  </nav>
  <b slot="language">
    <b slot="js">JavaScript adalah bahasa skrip</b>
    <b slot="rs">Rust adalah bahasa pemrograman tingkat sistem</b>
  </b>
  <b slot="os">
    <b slot="mac">macOS adalah sistem operasi grafis berbasis Unix</b>
    <b slot="win">Windows adalah sistem operasi yang dikembangkan oleh Microsoft</b>
  </b>
</c-tab-select>
```

## milik

- `key`: nama kunci yang digunakan untuk persistensi Penyimpanan lokal
- `value`: nilai yang dipilih saat ini

## Acara khusus

- `change`: Dipicu saat berpindah tab, `e.value` adalah nilai `value` dari opsi yang saat ini diaktifkan.