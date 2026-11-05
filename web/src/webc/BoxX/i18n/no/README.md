# Lag og vis et popup-lag med en lukkeknapp

- Funksjonell oppfordring for å opprette og vise popup-lag
- Innebygd lukkeknapp, klikk for å lukke og fjerne elementet fra DOM
- Trykk Esc for å ufokusere inndataboksen eller lukke popup-laget

## Bruk demoen

```html
<button>Klikk for å komme opp</button>

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


## Grensesnittparametere

Standard eksportfunksjon returnerer `HTMLDialogElement`, og stilklassene `Box` og `X` er lagt til dialogelementet.

## stilklasse

### `.Box.X`

Brukt på `dialog`-elementet, spesifiserer dialogstilen.

### `a.X`

Brukt på `a`-elementet, spesifiserer stilen for lukkeknappen.