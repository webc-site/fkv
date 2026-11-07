# A legördülő választódoboz és a gomb kombinációja

- Hajtsa össze a kijelölőmezőt és a gombot vízszintesen
- A kiválasztott állapot automatikus mentése a localStorage-ba
- Elfogja a gombkattintásokat és elküldi az egyéni eseményeket

## Használd a demót

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">1. lehetőség</option>
    <option value="option2">2. lehetőség</option>
  </c-select>
  <button>fut</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## ingatlan

- `key`: a localStorage fennmaradásához használt kulcsnév
- `value`: jelenleg kiválasztott érték

## esemény

- `submit`: A gombra kattintva küldik el, `e.value` az aktuálisan kiválasztott értéket hordozza