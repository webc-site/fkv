# Połączenie rozwijanego pola wyboru i przycisku

- Połącz pole wyboru i przycisk poziomo
- Automatycznie zapisuj wybrany stan w localStorage
- Przechwytuj kliknięcia przycisków i wysyłaj zdarzenia niestandardowe

## Skorzystaj z wersji demonstracyjnej

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">Opcja 1</option>
    <option value="option2">Opcja 2</option>
  </c-select>
  <button>uruchomić</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## nieruchomość

- `key`: nazwa klucza używana do trwałości localStorage
- `value`: aktualnie wybrana wartość

## wydarzenie

- `submit`: Wysyłane po kliknięciu przycisku, `e.value` przenosi aktualnie wybraną wartość