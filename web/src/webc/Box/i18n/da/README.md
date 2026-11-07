# Opret og vis popup-lag

- Funktionelt kald for at oprette og vise popup-lag
- Fjern automatisk elementer fra DOM, når de er lukket
- Forhindrer Esc-nøgleannulleringsadfærd

## Brug demoen

```html
<button>Klik for at pop op</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "Titel";
  description.textContent = "Tekst";
  button.className = "Btn Main";
  button.textContent = "OK";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Interface parametre

### Standard eksporterede funktioner

`Box()`

- **Returværdi**: `HTMLDialogElement`, dialogelementet i `Box` stilklasse er blevet tilføjet.

## stil klasse

### `.Box`

Gælder for `dialog`-elementet for at indstille positionering, centrering og sløring af baggrundsmaske.

### `.Lg`

En stilklasse anvendt på det indre indhold, som indstiller baggrundsfarven, afrundede hjørner, skygge og lodret layout og definerer layoutet af de indre `h3` og `p` .