# Stvorite i prikažite skočni sloj s gumbom za zatvaranje

- Funkcionalni poziv za stvaranje i prikaz skočnog sloja
- Ugrađeni gumb za zatvaranje, kliknite za zatvaranje i uklanjanje elementa iz DOM-a
- Pritisnite Esc da defokusirate okvir za unos ili zatvorite skočni sloj

## Koristite demo

```html
<button>Kliknite za iskakanje</button>

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


## Parametri sučelja

Zadana funkcija izvoza vraća `HTMLDialogElement`, a stilske klase `Box` i `X` dodane su dijaloškom elementu.

## stilska klasa

### `.Box.X`

Primijenjeno na element `dialog`, određuje stil dijaloga.

### `a.X`

Primijenjeno na element `a`, određuje stil gumba za zatvaranje.