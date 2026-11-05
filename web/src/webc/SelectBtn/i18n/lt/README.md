# Išskleidžiamojo pasirinkimo laukelio ir mygtuko derinys

- Sujunkite pasirinkimo laukelį ir mygtuką horizontaliai
- Automatiškai išsaugokite pasirinktą būseną vietinėje saugykloje
- Perimkite mygtukų paspaudimus ir išsiųskite pasirinktinius įvykius

## Naudokite demonstracinę versiją

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">1 variantas</option>
    <option value="option2">2 variantas</option>
  </c-select>
  <button>paleisti</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## nuosavybė

- `key`: rakto pavadinimas, naudojamas vietinei saugyklos išlikimui
- `value`: šiuo metu pasirinkta vertė

## įvykis

- `submit`: išsiunčiama paspaudus mygtuką, `e.value` turi šiuo metu pasirinktą reikšmę