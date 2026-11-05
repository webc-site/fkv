# Стварыце і пакажыце ўсплывальны пласт з кнопкай закрыцця

- Функцыянальны выклік для стварэння і адлюстравання ўсплывальнага слоя
- Убудаваная кнопка закрыцця, націсніце, каб закрыць і выдаліць элемент з DOM
- Націсніце Esc, каб расфакусаваць поле ўводу або закрыць усплывальны пласт

## Выкарыстоўвайце дэма

```html
<button>Націсніце, каб усплыць</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "тэкст";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## Параметры інтэрфейсу

Функцыя экспарту па змаўчанні вяртае `HTMLDialogElement`, а класы стыляў `Box` і `X` былі дададзены да дыялогавага элемента.

## клас стылю

### `.Box.X`

Ужываецца да элемента `dialog`, вызначае стыль дыялогу.

### `a.X`

Ужываецца да элемента `a`, вызначае стыль кнопкі закрыцця.