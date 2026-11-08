# Looge ja kuvage hüpikaknakiht

- Funktsionaalne kõne hüpikkihi loomiseks ja kuvamiseks
- Eemaldage sulgemisel DOM-ist elemendid automaatselt
- Vältige Esc-klahvi tühistamise käitumist

## Kasutage demo

```html
<button>Klõpsake hüpikakna kuvamiseks</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "Pealkiri";
  description.textContent = "Tekst";
  button.className = "Btn Main";
  button.textContent = "OK";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Liidese parameetrid

### Eksporditud vaikefunktsioonid

`Box()`

- **Tagastusväärtus**: `HTMLDialogElement`, stiiliklassi `Box` dialoogielement on lisatud.

## stiiliklass

### `.Box`

Kehtib elemendile `dialog` positsioneerimise, tsentreerimise ja taustamaski hägususe määramiseks.

### `.Lg`

Sisemisele sisule rakendatav stiiliklass, mis määrab taustavärvi, ümarad nurgad, varju- ja vertikaalpaigutuse ning sisemise `h3` ja `p` paigutuse.