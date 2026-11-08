# Kombinacija spustnega izbirnega polja in gumba

- Spojite izbirno polje in gumb vodoravno
- Samodejno shrani izbrano stanje v localStorage
- Prestrezi klike gumbov in odpošlji dogodke po meri

## Uporabite demo

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">Možnost 1</option>
    <option value="option2">Možnost 2</option>
  </c-select>
  <button>teči</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## premoženje

- `key`: ime ključa, ki se uporablja za obstojnost localStorage
- `value`: trenutno izbrana vrednost

## dogodek

- `submit`: Odposlano, ko kliknete gumb, `e.value` nosi trenutno izbrano vrednost