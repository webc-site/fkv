# Kombination aus Dropdown-Auswahlfeld und Schaltfläche

- Verbinden Sie das Auswahlfeld und die Schaltfläche horizontal
- Speichern Sie den ausgewählten Status automatisch in localStorage
- Schaltflächenklicks abfangen und benutzerdefinierte Ereignisse auslösen

## Nutzen Sie die Demo

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">Option 1</option>
    <option value="option2">Option 2</option>
  </c-select>
  <button>laufen</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## Eigentum

- `key`: Schlüsselname, der für die LocalStorage-Persistenz verwendet wird
- `value`: aktuell ausgewählter Wert

## Ereignis

- `submit`: Wird ausgelöst, wenn auf die Schaltfläche geklickt wird. `e.value` trägt den aktuell ausgewählten Wert