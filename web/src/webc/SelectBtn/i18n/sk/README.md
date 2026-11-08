# Kombinácia rozbaľovacieho poľa výberu a tlačidla

- Vodorovne spojte výberové pole a tlačidlo
- Automaticky uložiť vybraný stav do localStorage
- Zachyťte kliknutia na tlačidlá a odosielajte vlastné udalosti

## Použite demo

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">Možnosť 1</option>
    <option value="option2">Možnosť 2</option>
  </c-select>
  <button>behať</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## majetku

- `key`: názov kľúča používaný pre trvalosť localStorage
- `value`: aktuálne vybratá hodnota

## udalosť

- `submit`: Odošle sa po kliknutí na tlačidlo, `e.value` nesie aktuálne vybratú hodnotu