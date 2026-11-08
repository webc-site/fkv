# Paghimo ug pagpakita og popup layer nga adunay close button

- Functional nga tawag sa paghimo ug pagpakita sa popup layer
- Built-in nga close button, i-klik aron isara ug tangtangon ang elemento gikan sa DOM
- Pindota ang Esc aron ma-defocus ang input box o isira ang popup layer

## Gamita ang demo

```html
<button>I-klik aron mo-pop up</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "teksto";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## Mga parametro sa interface

Ang default export function mobalik `HTMLDialogElement`, ug ang `Box` ug `X` style classes gidugang sa dialog element.

## estilo nga klase

### `.Box.X`

Gipadapat sa `dialog` nga elemento, nagtino sa estilo sa dialogo.

### `a.X`

Gipadapat sa `a` nga elemento, nagtino sa estilo sa close button.