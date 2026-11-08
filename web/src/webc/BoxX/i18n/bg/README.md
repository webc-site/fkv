# Създайте и покажете изскачащ слой с бутон за затваряне

- Функционално повикване за създаване и показване на изскачащ слой
- Вграден бутон за затваряне, щракнете, за да затворите и премахнете елемента от DOM
- Натиснете Esc, за да разфокусирате полето за въвеждане или да затворите изскачащия слой

## Използвайте демонстрацията

```html
<button>Кликнете, за да изскочи</button>

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


## Параметри на интерфейса

Функцията за експортиране по подразбиране връща `HTMLDialogElement`, а стиловите класове `Box` и `X` са добавени към диалоговия елемент.

## стил клас

### `.Box.X`

Приложен към елемента `dialog`, указва стила на диалога.

### `a.X`

Приложен към елемента `a`, указва стила на бутона за затваряне.