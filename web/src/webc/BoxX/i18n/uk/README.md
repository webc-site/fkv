# Створіть і відобразіть спливаючий шар із кнопкою закриття

- Функціональний виклик для створення та відображення спливаючого шару
- Вбудована кнопка закриття, натисніть, щоб закрити та видалити елемент із DOM
- Натисніть Esc, щоб розфокусувати поле введення або закрити спливаючий шар

## Використовуйте демо

```html
<button>Натисніть, щоб спливаюче вікно</button>

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


## Параметри інтерфейсу

Функція експорту за умовчанням повертає `HTMLDialogElement`, а класи стилів `Box` і `X` додано до елемента діалогу.

## клас стилю

### `.Box.X`

Застосовується до елемента `dialog`, визначає стиль діалогу.

### `a.X`

Застосовується до елемента `a`, визначає стиль кнопки закриття.