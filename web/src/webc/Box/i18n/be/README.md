# Стварэнне і адлюстраванне ўсплывальнага пласта

- Функцыянальны выклік для стварэння і адлюстравання ўсплывальнага слоя
- Аўтаматычна выдаляць элементы з DOM пры закрыцці
- Прадухіліць паводзіны адмены клавішы Esc

## Выкарыстоўвайце дэма

```html
<button>Націсніце, каб усплыць</button>

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
  description.textContent = "Тэкст";
  button.className = "Btn Main";
  button.textContent = "ОК";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Параметры інтэрфейсу

### Экспартаваныя функцыі па змаўчанні

`Box()`

- **Вяртанае значэнне**: `HTMLDialogElement`, быў дададзены элемент дыялогу класа стылю `Box`.

## клас стылю

### `.Box`

Прымяняецца да элемента `dialog` для ўстаноўкі пазіцыянавання, цэнтравання і размыцця фонавай маскі.

### `.Lg`

Клас стылю, які прымяняецца да ўнутранага змесціва, усталёўваючы колер фону, закругленыя куты, цень і вертыкальнае размяшчэнне, а таксама вызначаючы размяшчэнне ўнутраных `h3` і `p` .