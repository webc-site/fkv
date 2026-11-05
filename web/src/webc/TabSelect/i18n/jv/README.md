# Kaca tab kanthi pilihan gulung mudhun

- Ndhukung nanem kothak pilihan gulung-mudhun jembar adaptif (`<c-select>`) ing label, sing mung bakal ditampilake nalika kaca tab saiki diaktifake.
- Spesifikasi struktural: Elemen anak nggunakake `nav > a` minangka navigasi label, lan unsur anak langsung `b` dadi area isi.
- Kanthi otomatis nggandhengake tag `a[value]`, `a` karo `c-select` (adhedhasar atribut nilai grup lan nilai pilihan sing dipilih) lan simpul isi `b[slot]`.
- Ndhukung kanthi otomatis tetep item sing saiki diaktifake kanggo `localStorage` liwat atribut `key`.
- Ndhukung otomatis nyimpen lan mulihake sub-pilihan sajarah kanggo saben grup kanggo `localStorage` nggunakake `key + '/' + tab`.
- Ndhukung nemtokake tab aktif liwat atribut `value`.

## Gunakake demo

```html
<c-tab-select key="my-tab-select-key" value="language/js">
  <nav>
    <a value="language">
      <span>basa</span>
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
    <b slot="js">JavaScript minangka basa skrip</b>
    <b slot="rs">Rust minangka basa pemrograman tingkat sistem</b>
  </b>
  <b slot="os">
    <b slot="mac">macOS minangka sistem operasi grafis berbasis Unix</b>
    <b slot="win">Windows minangka sistem operasi sing dikembangake dening Microsoft</b>
  </b>
</c-tab-select>
```

## properti

- `key`: jeneng kunci sing digunakake kanggo kegigihan localStorage
- `value`: nilai sing dipilih saiki

## acara adat

- `change`: Dipicu nalika ngoper tab, `e.value` minangka nilai `value` saka opsi sing saiki diaktifake.