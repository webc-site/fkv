# Комбінація випадаючого поля вибору та кнопки

- З’єднайте поле вибору та кнопку горизонтально
- Автоматично зберігати вибраний стан у localStorage
- Перехоплювати натискання кнопок і відправляти спеціальні події

## Використовуйте демо

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">Варіант 1</option>
    <option value="option2">Варіант 2</option>
  </c-select>
  <button>бігти</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## власність

- `key`: назва ключа, що використовується для збереження локального сховища
- `value`: поточне вибране значення

## подія

- `submit`: надсилається, коли натиснуто кнопку, `e.value` містить поточне вибране значення