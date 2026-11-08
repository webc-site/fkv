# Gabungan kotak pilihan juntai bawah dan butang

- Sambungkan kotak pilihan dan butang secara mendatar
- Simpan keadaan yang dipilih secara automatik ke localStorage
- Klik butang memintas dan menghantar acara tersuai

## Gunakan demo

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">Pilihan 1</option>
    <option value="option2">Pilihan 2</option>
  </c-select>
  <button>lari</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## harta benda

- `key`: nama kunci yang digunakan untuk kegigihan localStorage
- `value`: nilai yang dipilih pada masa ini

## peristiwa

- `submit`: Dihantar apabila butang diklik, `e.value` membawa nilai yang dipilih pada masa ini