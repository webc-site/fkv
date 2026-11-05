# Kombinasyon sa drop-down nga kahon sa pagpili ug buton

- Idugtong ang kahon sa pagpili ug ang buton nga pinahigda
- Awtomatikong i-save ang pinili nga estado sa localStorage
- Pag-intercept sa mga pag-klik sa buton ug ipadala ang naandan nga mga panghitabo

## Gamita ang demo

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">Opsyon 1</option>
    <option value="option2">Opsyon 2</option>
  </c-select>
  <button>dagan</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## kabtangan

- `key`: yawe nga ngalan nga gigamit alang sa pagpadayon sa localStorage
- `value`: karon pinili nga bili

## panghitabo

- `submit`: Gipadala sa dihang gi-klik ang buton, `e.value` nagdala sa gipili karon nga bili