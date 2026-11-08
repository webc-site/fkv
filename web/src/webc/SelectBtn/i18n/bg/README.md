# Комбинация от падащо поле за избор и бутон

- Свържете полето за избор и бутона хоризонтално
- Автоматично запазване на избраното състояние в localStorage
- Прихващане на кликвания върху бутони и изпращане на персонализирани събития

## Използвайте демонстрацията

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">Вариант 1</option>
    <option value="option2">Вариант 2</option>
  </c-select>
  <button>бягам</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## собственост

- `key`: име на ключ, използвано за постоянство на localStorage
- `value`: текущо избрана стойност

## събитие

- `submit`: Изпраща се при щракване върху бутона, `e.value` носи текущо избраната стойност