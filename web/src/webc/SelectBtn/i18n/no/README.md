# Kombinasjon av rullegardinvalgboks og knapp

- Skjøt valgboksen og knappen horisontalt
- Lagre den valgte tilstanden automatisk til localStorage
- Avskjær knappeklikk og send egendefinerte hendelser

## Bruk demoen

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">Alternativ 1</option>
    <option value="option2">Alternativ 2</option>
  </c-select>
  <button>løp</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## eiendom

- `key`: nøkkelnavn brukt for lokal lagrings-vedholdenhet
- `value`: gjeldende valgt verdi

## hendelse

- `submit`: Sendes når knappen klikkes, `e.value` har den gjeldende valgte verdien