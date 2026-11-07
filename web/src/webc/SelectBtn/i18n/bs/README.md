# Kombinacija padajućeg okvira za izbor i dugmeta

- Spojite okvir za odabir i dugme vodoravno
- Automatski spremite odabrano stanje u localStorage
- Presretnite klikove na dugme i otpremite prilagođene događaje

## Koristite demo

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">Opcija 1</option>
    <option value="option2">Opcija 2</option>
  </c-select>
  <button>trči</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## imovine

- `key`: ime ključa koji se koristi za postojanost localStorage
- `value`: trenutno odabrana vrijednost

## događaj

- `submit`: Šalje se kada se klikne na dugme, `e.value` nosi trenutno odabranu vrijednost