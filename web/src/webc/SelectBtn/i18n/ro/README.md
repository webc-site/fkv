# Combinație de casetă de selecție drop-down și buton

- Îmbină caseta de selecție și butonul pe orizontală
- Salvați automat starea selectată în localStorage
- Interceptați clicurile pe buton și trimiteți evenimente personalizate

## Utilizați demonstrația

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">Opțiunea 1</option>
    <option value="option2">Opțiunea 2</option>
  </c-select>
  <button>alerga</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## proprietate

- `key`: numele cheii folosit pentru persistența localStorage
- `value`: valoarea selectată în prezent

## eveniment

- `submit`: expediat când se face clic pe butonul, `e.value` poartă valoarea selectată curent