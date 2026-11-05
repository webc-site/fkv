# Creu ac arddangos haenen naid

- Galwad swyddogaethol i greu ac arddangos haenen naid
- Tynnwch elfennau o'r DOM yn awtomatig pan fydd ar gau
- Atal ymddygiad canslo allwedd Esc

## Defnyddiwch y demo

```html
<button>Cliciwch i pop i fyny</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "Teitl";
  description.textContent = "Testun";
  button.className = "Btn Main";
  button.textContent = "Iawn";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Paramedrau rhyngwyneb

### Swyddogaethau diofyn wedi'u hallforio

`Box()`

- **Gwerth dychwelyd**: `HTMLDialogElement`, mae'r elfen ymgom o `Box` dosbarth arddull wedi'i hychwanegu.

## dosbarth arddull

### `.Box`

Yn berthnasol i'r elfen `dialog` i osod lleoliad, canoli, a niwl mwgwd cefndir.

### `.Lg`

Roedd dosbarth arddull yn berthnasol i'r cynnwys mewnol, gan osod y lliw cefndir, corneli crwn, cysgod a gosodiad fertigol, a diffinio gosodiad y `h3` a `p` mewnol.