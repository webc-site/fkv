# Создание и отображение всплывающего слоя

- Функциональный вызов для создания и отображения всплывающего слоя
- Автоматически удалять элементы из DOM при закрытии
- Запретить поведение отмены клавиши Esc

## Используйте демо-версию

```html
<button>Нажмите, чтобы открыть всплывающее окно</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "Заголовок";
  описание.textContent = "Текст";
  button.className = "Btn Main";
  button.textContent = "ОК";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Параметры интерфейса

### Экспортируемые функции по умолчанию

`Box()`

- **Возвращаемое значение**: `HTMLDialogElement`, добавлен элемент диалога класса стиля `Box`.

## класс стиля

### `.Box`

Применяется к элементу `dialog` для установки позиционирования, центрирования и размытия маски фона.

### `.Lg`

Класс стиля, применяемый к внутреннему содержимому, устанавливающий цвет фона, закругленные углы, тень и вертикальное расположение, а также определяющий макет внутренних `h3` и `p` .