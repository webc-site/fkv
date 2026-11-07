# Ашылмалы таңдау жолағы мен түйменің тіркесімі

- Таңдау жолағын және түймені көлденеңінен жалғаңыз
- Таңдалған күйді localStorage ішіне автоматты түрде сақтаңыз
- Түймені басу және реттелетін оқиғаларды жіберу

## Демонстрацияны пайдаланыңыз

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">1-нұсқа</option>
    <option value="option2">2-нұсқа</option>
  </c-select>
  <button>жүгіру</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## мүлік

- `key`: localStorage тұрақтылығы үшін пайдаланылатын кілт атауы
- `value`: ағымдағы таңдалған мән

## оқиға

- `submit`: түйме басылғанда жіберіледі, `e.value` ағымдағы таңдалған мәнді тасымалдайды