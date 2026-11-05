# Paghimo ug ipakita ang popup layer

- Functional nga tawag sa paghimo ug pagpakita sa popup layer
- Awtomatikong tangtangon ang mga elemento gikan sa DOM kung sirado
- Paglikay sa pamatasan sa pagkansela sa yawe sa Esc

## Gamita ang demo

```html
<button>I-klik aron mo-pop up</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "Titulo";
  description.textContent = "Teksto";
  button.className = "Btn Main";
  button.textContent = "OK";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Mga parametro sa interface

### Default nga gi-eksport nga mga gimbuhaton

`Box()`

- **Return value**: `HTMLDialogElement`, ang dialog element sa `Box` style class gidugang.

## estilo nga klase

### `.Box`

Naaplikar sa `dialog` nga elemento para itakda ang positioning, centering, ug background mask blur.

### `.Lg`

Usa ka klase sa estilo nga gipadapat sa sulod nga sulod, nagbutang sa kolor sa background, mga lingin nga mga kanto, anino ug bertikal nga layout, ug naghubit sa layout sa sulod nga `h3` ug `p` .