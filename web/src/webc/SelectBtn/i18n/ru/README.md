# Комбинация раскрывающегося списка выбора и кнопки

- Соедините рамку выбора и кнопку горизонтально
- Автоматически сохранять выбранное состояние в localStorage
- Перехватывайте нажатия кнопок и отправляйте пользовательские события

## Используйте демо-версию

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">Вариант 1</option>
    <option value="option2">Вариант 2</option>
  </c-select>
  <button>бегать</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## свойство

- `key`: имя ключа, используемое для сохранения localStorage.
- `value`: выбранное в данный момент значение.

## событие

- `submit`: отправляется при нажатии кнопки. `e.value` содержит выбранное в данный момент значение.