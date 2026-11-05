# Combinatie van vervolgkeuzemenu en knop

- Verdeel het selectievak en de knop horizontaal
- Sla de geselecteerde status automatisch op in localStorage
- Onderschep klikken op knoppen en verzend aangepaste gebeurtenissen

## Gebruik de demo

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">Optie 1</option>
    <option value="option2">Optie 2</option>
  </c-select>
  <button>loop</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## eigendom

- `key`: sleutelnaam gebruikt voor localStorage-persistentie
- `value`: momenteel geselecteerde waarde

## evenement

- `submit`: Wordt verzonden wanneer op de knop wordt geklikt, `e.value` bevat de momenteel geselecteerde waarde