# Kombinacija padajućeg okvira za odabir i gumba

- Vodoravno spojite okvir za odabir i gumb
- Automatski spremi odabrano stanje u localStorage
- Presretanje klikova gumba i slanje prilagođenih događaja

## Koristite demo

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">Opcija 1</option>
    <option value="option2">opcija 2</option>
  </c-select>
  <button>trčanje</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## imovine

- `key`: naziv ključa koji se koristi za postojanost localStorage
- `value`: trenutno odabrana vrijednost

## događaj

- `submit`: Šalje se kada se klikne gumb, `e.value` nosi trenutno odabranu vrijednost