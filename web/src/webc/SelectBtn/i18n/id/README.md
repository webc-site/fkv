# Kombinasi kotak pilihan drop-down dan tombol

- Sambungkan kotak pilihan dan tombol secara horizontal
- Secara otomatis menyimpan status yang dipilih ke Penyimpanan lokal
- Mencegat klik tombol dan mengirimkan acara khusus

## Gunakan demonya

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">Pilihan 1</option>
    <option value="option2">pilihan 2</option>
  </c-select>
  <button>berlari</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## milik

- `key`: nama kunci yang digunakan untuk persistensi Penyimpanan lokal
- `value`: nilai yang dipilih saat ini

## peristiwa

- `submit`: Dikirim ketika tombol diklik, `e.value` membawa nilai yang dipilih saat ini