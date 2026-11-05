# Kombinasi kothak pilihan gulung mudhun lan tombol

- Sambungake kothak pilihan lan tombol kanthi horisontal
- Simpen status sing dipilih kanthi otomatis menyang localStorage
- Klik tombol nyegat lan ngirim acara khusus

## Gunakake demo

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">Pilihan 1</option>
    <option value="option2">Pilihan 2</option>
  </c-select>
  <button>mlayu</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## properti

- `key`: jeneng kunci sing digunakake kanggo kegigihan localStorage
- `value`: nilai sing dipilih saiki

## acara

- `submit`: Dikirim nalika tombol diklik, `e.value` nggawa nilai sing saiki dipilih