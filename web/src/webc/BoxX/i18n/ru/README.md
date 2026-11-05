# Создайте и отобразите всплывающий слой с кнопкой закрытия.

- Функциональный вызов для создания и отображения всплывающего слоя
- Встроенная кнопка закрытия: нажмите, чтобы закрыть и удалить элемент из DOM.
- Нажмите Esc, чтобы расфокусировать поле ввода или закрыть всплывающий слой.

## Используйте демо-версию

```html
<button>Нажмите, чтобы открыть всплывающее окно</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "текст";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## Параметры интерфейса

Функция экспорта по умолчанию возвращает `HTMLDialogElement`, а к элементу диалога добавлены классы стилей `Box` и `X`.

## класс стиля

### `.Box.X`

Применяется к элементу `dialog` и определяет стиль диалога.

### `a.X`

Применяется к элементу `a` и определяет стиль кнопки закрытия.