# Looge ja kuvage sulgemisnupuga hüpikaknakiht

- Funktsionaalne kõne hüpikkihi loomiseks ja kuvamiseks
- Sisseehitatud sulgemisnupp, klõpsake elemendi sulgemiseks ja eemaldamiseks DOM-ist
- Sisestuskasti defokuseerimiseks või hüpikakna sulgemiseks vajutage Esc

## Kasutage demo

```html
<button>Klõpsake hüpikakna kuvamiseks</button>

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


## Liidese parameetrid

Ekspordi vaikefunktsioon tagastab `HTMLDialogElement` ning dialoogielemendile on lisatud stiiliklassid `Box` ja `X`.

## stiiliklass

### `.Box.X`

Rakendatakse elemendile `dialog`, määrab dialoogi stiili.

### `a.X`

Elemendile `a` rakendatuna määrab sulgemisnupu stiili.