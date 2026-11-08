# Створення та відображення спливаючого шару

- Функціональний виклик для створення та відображення спливаючого шару
- Автоматично видаляти елементи з DOM після закриття
- Запобігання поведінці скасування клавіші Esc

## Використовуйте демо

```html
<button>Натисніть, щоб спливаюче вікно</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "Назва";
  description.textContent = "Текст";
  button.className = "Btn Main";
  button.textContent = "ОК";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Параметри інтерфейсу

### Експортовані функції за замовчуванням

`Box()`

- **Повернене значення**: `HTMLDialogElement`, додано діалоговий елемент класу стилю `Box`.

## клас стилю

### `.Box`

Застосовується до елемента `dialog` для налаштування позиціонування, центрування та розмиття фонової маски.

### `.Lg`

Клас стилю, застосований до внутрішнього вмісту, встановлюючи колір фону, округлені кути, тінь і вертикальне розташування, а також визначаючи макет внутрішніх `h3` і `p`.