# Maak en toon een pop-uplaag met een sluitknop

- Functionele aanroep voor het maken en weergeven van een pop-uplaag
- Ingebouwde sluitknop, klik om het element te sluiten en uit de DOM te verwijderen
- Druk op Esc om het invoervak ​​onscherp te maken of de pop-uplaag te sluiten

## Gebruik de demo

```html
<button>Klik om te verschijnen</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "tekst";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## Interfaceparameters

De standaard exportfunctie retourneert `HTMLDialogElement` en de stijlklassen `Box` en `X` zijn aan het dialoogelement toegevoegd.

## stijl klasse

### `.Box.X`

Toegepast op het `dialog` element, specificeert de dialoogstijl.

### `a.X`

Toegepast op het `a` element, specificeert de sluitknopstijl.