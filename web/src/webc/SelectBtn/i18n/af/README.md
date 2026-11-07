# Kombinasie van aftreklys seleksie boks en knoppie

- Verbind die keuseblokkie en knoppie horisontaal
- Stoor die geselekteerde toestand outomaties in localStorage
- Onderskep knoppie klik en stuur pasgemaakte gebeurtenisse

## Gebruik die demo

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">Opsie 1</option>
    <option value="option2">Opsie 2</option>
  </c-select>
  <button>hardloop</button>
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

- `key`: sleutelnaam gebruik vir localStorage-volharding
- `value`: tans geselekteerde waarde

## gebeurtenis

- `submit`: Gestuur wanneer die knoppie geklik word, dra `e.value` die tans geselekteerde waarde