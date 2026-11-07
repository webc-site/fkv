# Pop-uplaag maken en weergeven

- Functionele aanroep voor het maken en weergeven van een pop-uplaag
- Verwijder automatisch elementen uit de DOM wanneer deze gesloten is
- Voorkom gedrag bij het annuleren van Esc-sleutels

## Gebruik de demo

```html
<button>Klik om te verschijnen</button>

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

## Interfaceparameters

### Standaard geëxporteerde functies

`Box()`

- **Retourwaarde**: `HTMLDialogElement`, het dialoogelement van de stijlklasse `Box` is toegevoegd.

## stijl klasse

### `.Box`

Is van toepassing op het `dialog`-element om de positionering, centrering en vervaging van het achtergrondmasker in te stellen.

### `.Lg`

Een stijlklasse die wordt toegepast op de binnenste inhoud, die de achtergrondkleur, afgeronde hoeken, schaduw en verticale lay-out instelt, en de lay-out van de binnenste `h3` en `p` definieert.