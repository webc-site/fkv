# Kombinatioun vun Dropdown Auswiel Këscht a Knäppchen

- Splice d'Selektiounskëscht a Knäppchen horizontal
- Späichert automatesch de gewielte Staat op localStorage
- Offangen Knäppchen klickt a verschéckt personaliséiert Eventer

## Benotzt d'Demo

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">Optioun 1</option>
    <option value="option2">Optioun 2</option>
  </c-select>
  <button>lafen</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## Propriétéit

- `key`: Schlësselnumm benotzt fir d'LokalStorage Persistenz
- `value`: aktuell gewielte Wäert

## Event

- `submit`: Verschéckt wann de Knäppchen geklickt gëtt, `e.value` dréit den aktuell gewielte Wäert