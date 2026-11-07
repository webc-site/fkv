# Combinació de quadre de selecció desplegable i botó

- Empalmeu el quadre de selecció i el botó horitzontalment
- Desa automàticament l'estat seleccionat a localStorage
- Interceptar els clics dels botons i enviar esdeveniments personalitzats

## Utilitzeu la demostració

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">Opció 1</option>
    <option value="option2">Opció 2</option>
  </c-select>
  <button>córrer</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## propietat

- `key`: nom de clau utilitzat per a la persistència de localStorage
- `value`: valor seleccionat actualment

## esdeveniment

- `submit`: enviat quan es fa clic al botó, `e.value` porta el valor seleccionat actualment