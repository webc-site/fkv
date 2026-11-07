# Opret og vis et pop op-lag med en lukkeknap

- Funktionelt kald for at oprette og vise popup-lag
- Indbygget lukkeknap, klik for at lukke og fjerne elementet fra DOM
- Tryk på Esc for at defokusere inputfeltet eller lukke pop op-laget

## Brug demoen

```html
<button>Klik for at pop op</button>

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


## Interface parametre

Standard eksportfunktionen returnerer `HTMLDialogElement`, og stilklasserne `Box` og `X` er blevet tilføjet til dialogelementet.

## stil klasse

### `.Box.X`

Anvendt på `dialog`-elementet, angiver dialogens stil.

### `a.X`

Anvendt på `a`-elementet, angiver luk-knappens stil.