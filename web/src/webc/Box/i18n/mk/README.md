# Креирајте и прикажете скокачки слој

- Функционален повик за креирање и прикажување на скокачки слој
- Автоматски отстранете ги елементите од DOM кога е затворен
- Спречете однесување за откажување на клучот Esc

## Користете го демо

```html
<button>Кликнете за да се појави</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "Наслов";
  description.textContent = "Текст";
  button.className = "Btn Main";
  button.textContent = "Во ред";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Параметри на интерфејсот

### Стандардно извезени функции

`Box()`

- **Повратна вредност**: `HTMLDialogElement`, додаден е елементот за дијалог од класата стил `Box`.

## класа стил

### `.Box`

Се применува на елементот `dialog` за поставување позиционирање, центрирање и замаглување на маската на заднината.

### `.Lg`

Класа на стил се применува на внатрешната содржина, поставувајќи ја бојата на позадината, заоблените агли, сенката и вертикалниот распоред и дефинирајќи го распоредот на внатрешниот `h3` и `p` .