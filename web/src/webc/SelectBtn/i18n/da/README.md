# Kombination af drop-down valgboks og knap

- Splej markeringsboksen og knappen vandret
- Gem automatisk den valgte tilstand til localStorage
- Opsnappe knapklik og afsende tilpassede begivenheder

## Brug demoen

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">Mulighed 1</option>
    <option value="option2">Mulighed 2</option>
  </c-select>
  <button>løbe</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## ejendom

- `key`: nøglenavn, der bruges til localStorage persistens
- `value`: aktuelt valgt værdi

## tilfælde

- `submit`: Sendes når der klikkes på knappen, `e.value` bærer den aktuelt valgte værdi