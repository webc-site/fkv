# Combinazione di casella di selezione a discesa e pulsante

- Unisci la casella di selezione e il pulsante orizzontalmente
- Salva automaticamente lo stato selezionato in localStorage
- Intercetta i clic sui pulsanti e invia eventi personalizzati

## Utilizza la demo

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">Opzione 1</option>
    <option value="option2">Opzione 2</option>
  </c-select>
  <button>correre</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## proprietà

- `key`: nome della chiave utilizzato per la persistenza localStorage
- `value`: valore attualmente selezionato

## evento

- `submit`: inviato quando si fa clic sul pulsante, `e.value` porta il valore attualmente selezionato