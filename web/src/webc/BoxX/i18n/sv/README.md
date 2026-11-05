# Skapa och visa ett popup-lager med en stängningsknapp

- Funktionellt anrop för att skapa och visa popup-lager
- Inbyggd stängningsknapp, klicka för att stänga och ta bort elementet från DOM
- Tryck på Esc för att ofokusera inmatningsrutan eller stänga popup-lagret

## Använd demon

```html
<button>Klicka för att dyka upp</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "text";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## Gränssnittsparametrar

Standardexportfunktionen returnerar `HTMLDialogElement` och stilklasserna `Box` och `X` har lagts till i dialogelementet.

## stilklass

### `.Box.X`

Tillämpas på `dialog`-elementet, anger dialogstilen.

### `a.X`

Tillämpas på `a`-elementet, anger stilen för stängningsknappen.