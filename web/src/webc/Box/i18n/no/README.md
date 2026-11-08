# Opprett og vis popup-lag

- Funksjonell oppfordring for å opprette og vise popup-lag
- Fjern automatisk elementer fra DOM når de er lukket
- Forhindre oppførsel for kansellering av Esc-nøkler

## Bruk demoen

```html
<button>Klikk for å komme opp</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "Tittel";
  description.textContent = "Tekst";
  button.className = "Btn Main";
  button.textContent = "OK";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Grensesnittparametere

### Standard eksporterte funksjoner

`Box()`

- **Returverdi**: `HTMLDialogElement`, dialogelementet til `Box` stilklassen er lagt til.

## stilklasse

### `.Box`

Gjelder `dialog`-elementet for å angi posisjonering, sentrering og uskarphet for bakgrunnsmaske.

### `.Lg`

En stilklasse brukt på det indre innholdet, angir bakgrunnsfargen, avrundede hjørner, skygge og vertikal layout, og definerer layouten til de indre `h3` og `p` .