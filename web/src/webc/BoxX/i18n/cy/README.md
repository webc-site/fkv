# Creu ac arddangos haenen naid gyda botwm cau

- Galwad swyddogaethol i greu ac arddangos haenen naid
- Botwm cau wedi'i gynnwys, cliciwch i gau a thynnu'r elfen o'r DOM
- Pwyswch Esc i ddadffocysu'r blwch mewnbwn neu gau'r haenen naid

## Defnyddiwch y demo

```html
<button>Cliciwch i pop i fyny</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "testun";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## Paramedrau rhyngwyneb

Mae'r ffwythiant allforio rhagosodedig yn dychwelyd `HTMLDialogElement`, ac mae'r dosbarthiadau arddull `Box` a `X` wedi'u hychwanegu at yr elfen ymgom.

## dosbarth arddull

### `.Box.X`

Wedi'i gymhwyso i'r elfen `dialog`, mae'n pennu arddull yr ymgom.

### `a.X`

Wedi'i gymhwyso i'r elfen `a`, mae'n pennu arddull y botwm cau.