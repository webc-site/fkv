# Kombinazzjoni ta 'kaxxa ta' għażla drop-down u buttuna

- Splice l-kaxxa tal-għażla u l-buttuna orizzontalment
- Issejvja awtomatikament l-istat magħżul għal localStorage
- Interċetta klikks tal-buttuna u ibgħat avvenimenti tad-dwana

## Uża d-demo

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">Għażla 1</option>
    <option value="option2">Għażla 2</option>
  </c-select>
  <button>run</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## proprjetà

- `key`: isem ewlieni użat għall-persistenza tal-LokalStorage
- `value`: valur magħżul bħalissa

## avveniment

- `submit`: Mibgħuta meta l-buttuna tiġi kklikkjata, `e.value` iġorr il-valur magħżul bħalissa