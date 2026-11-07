# Halaman tab dengan pilihan lungsur turun

- Menyokong membenamkan kotak pilihan lungsur turun lebar adaptif (`<c-select>`) dalam label, yang hanya akan dipaparkan apabila halaman tab semasa diaktifkan.
- Spesifikasi struktur: Elemen anak menggunakan `nav > a` sebagai navigasi label dan elemen anak langsung `b` berfungsi sebagai kawasan kandungan.
- Mengaitkan teg `a[value]` secara automatik, `a` dengan `c-select` (berdasarkan atribut nilai kumpulan dan nilai pilihan yang dipilih) dan nod kandungan `b[slot]`.
- Menyokong secara automatik mengekalkan item yang sedang diaktifkan ke `localStorage` melalui atribut `key`.
- Menyokong penyimpanan automatik dan pemulihan sub-pilihan sejarah untuk setiap kumpulan kepada `localStorage` menggunakan `key + '/' + tab`.
- Menyokong menentukan tab aktif melalui atribut `value`.

## Gunakan demo

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
    <b slot="js">JavaScript ialah bahasa skrip</b>
    <b slot="rs">Rust ialah bahasa pengaturcaraan peringkat sistem</b>
  </b>
  <b slot="os">
    <b slot="mac">macOS ialah sistem pengendalian grafik berasaskan Unix</b>
    <b slot="win">Windows ialah sistem pengendalian yang dibangunkan oleh Microsoft</b>
  </b>
</c-tab-select>
```

## harta benda

- `key`: nama kunci yang digunakan untuk kegigihan localStorage
- `value`: nilai yang dipilih pada masa ini

## Acara tersuai

- `change`: Dicetuskan apabila menukar tab, `e.value` ialah nilai `value` bagi pilihan yang sedang diaktifkan.