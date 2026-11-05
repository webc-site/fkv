# Cyfuniad o gwymplen blwch dewis a botwm

- Rhowch y blwch dewis a'r botwm yn llorweddol
- Arbedwch y cyflwr a ddewiswyd yn awtomatig i localStorage
- Rhyng-gipio cliciau botwm ac anfon digwyddiadau arferiad

## Defnyddiwch y demo

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">Opsiwn 1</option>
    <option value="option2">Opsiwn 2</option>
  </c-select>
  <button>rhedeg</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## eiddo

- `key`: enw allweddol a ddefnyddir ar gyfer dyfalbarhad storio lleol
- `value`: gwerth a ddewiswyd ar hyn o bryd

## digwyddiad

- `submit`: Wedi'i anfon pan glicir y botwm, mae `e.value` yn cario'r gwerth a ddewiswyd ar hyn o bryd