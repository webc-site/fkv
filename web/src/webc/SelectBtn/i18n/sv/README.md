# Kombination av rullgardinsvalsruta och knapp

- Skarva markeringsrutan och knappen horisontellt
- Spara automatiskt det valda tillståndet till localStorage
- Avlyssna knappklick och skicka anpassade händelser

## Använd demon

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">Alternativ 1</option>
    <option value="option2">Alternativ 2</option>
  </c-select>
  <button>sikt</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## egendom

- `key`: nyckelnamn som används för lokallagringsbeständighet
- `value`: för närvarande valt värde

## händelse

- `submit`: Skickas när knappen klickas, `e.value` bär det för närvarande valda värdet