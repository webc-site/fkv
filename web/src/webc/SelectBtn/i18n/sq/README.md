# Kombinimi i kutisë dhe butonit të zgjedhjes me zbritje

- Lidhni kutinë e përzgjedhjes dhe butonin horizontalisht
- Ruani automatikisht gjendjen e zgjedhur në localStorage
- Ndërprerja e klikimeve të butonit dhe dërgimi i ngjarjeve të personalizuara

## Përdorni demonstrimin

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">Opsioni 1</option>
    <option value="option2">Opsioni 2</option>
  </c-select>
  <button>vraponi</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## prone

- `key`: emri kyç i përdorur për qëndrueshmërinë e memorjes lokale
- `value`: vlera e zgjedhur aktualisht

## ngjarje

- `submit`: Dërgohet kur klikohet butoni, `e.value` mbart vlerën e zgjedhur aktualisht