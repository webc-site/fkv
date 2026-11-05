# Kombinace rozevíracího výběrového pole a tlačítka

- Vodorovně spojte pole výběru a tlačítko
- Automaticky uložit vybraný stav do localStorage
- Zachyťte kliknutí na tlačítka a odešlete vlastní události

## Použijte demo

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">Možnost 1</option>
    <option value="option2">Možnost 2</option>
  </c-select>
  <button>běh</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## vlastnictví

- `key`: název klíče používaný pro persistenci localStorage
- `value`: aktuálně vybraná hodnota

## událost

- `submit`: Odešle se po kliknutí na tlačítko, `e.value` nese aktuálně vybranou hodnotu