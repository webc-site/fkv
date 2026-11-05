# Създайте и покажете изскачащ слой

- Функционално повикване за създаване и показване на изскачащ слой
- Автоматично премахване на елементи от DOM, когато е затворен
- Предотвратете поведението при анулиране на клавиш Esc

## Използвайте демонстрацията

```html
<button>Кликнете, за да изскочи</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "Заглавие";
  description.textContent = "Текст";
  button.className = "Btn Main";
  button.textContent = "ОК";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Параметри на интерфейса

### Експортирани по подразбиране функции

`Box()`

- **Връщана стойност**: `HTMLDialogElement`, добавен е диалоговият елемент на стиловия клас `Box`.

## стил клас

### `.Box`

Прилага се към елемента `dialog` за задаване на позициониране, центриране и замъгляване на фоновата маска.

### `.Lg`

Клас стил, приложен към вътрешното съдържание, задаващ цвета на фона, заоблени ъгли, сянка и вертикално оформление и определящ оформлението на вътрешните `h3` и `p`.