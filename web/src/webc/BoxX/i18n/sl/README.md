# Ustvarite in prikažite pojavno plast z gumbom za zapiranje

- Funkcionalni klic za ustvarjanje in prikaz pojavne plasti
- Vgrajen gumb za zapiranje, kliknite, da zaprete in odstranite element iz DOM
- Pritisnite Esc, da razostrite polje za vnos ali zaprete pojavno plast

## Uporabite demo

```html
<button>Kliknite za pojavno okno</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "besedilo";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## Parametri vmesnika

Privzeta izvozna funkcija vrne `HTMLDialogElement`, slogovna razreda `Box` in `X` pa sta bila dodana elementu pogovornega okna.

## stilski razred

### `.Box.X`

Uporabljeno za element `dialog`, določa slog pogovornega okna.

### `a.X`

Uporabljeno za element `a`, določa slog gumba za zapiranje.