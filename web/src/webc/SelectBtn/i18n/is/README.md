# Sambland af fellivalkassa og hnappi

- Skerið valboxið og hnappinn lárétt
- Vistaðu valið ástand sjálfkrafa í localStorage
- Hlera smelli á hnappa og senda sérsniðna atburði

## Notaðu kynninguna

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">Valkostur 1</option>
    <option value="option2">Valkostur 2</option>
  </c-select>
  <button>hlaupa</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## eign

- `key`: lykilheiti notað fyrir staðbundið geymsluþol
- `value`: valið gildi

## atburður

- `submit`: Sendur þegar smellt er á hnappinn, `e.value` ber núverandi valið gildi