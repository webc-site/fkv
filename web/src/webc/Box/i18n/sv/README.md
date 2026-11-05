# Skapa och visa popup-lager

- Funktionellt anrop för att skapa och visa popup-lager
- Ta automatiskt bort element från DOM när det är stängt
- Förhindra Esc-nyckelavbrytningsbeteende

## Använd demon

```html
<button>Klicka för att dyka upp</button>

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
  description.textContent = "Text";
  button.className = "Btn Main";
  button.textContent = "OK";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Gränssnittsparametrar

### Exporterade standardfunktioner

`Box()`

- **Returvärde**: `HTMLDialogElement`, dialogelementet för `Box` stilklass har lagts till.

## stilklass

### `.Box`

Gäller `dialog`-elementet för att ställa in positionering, centrering och bakgrundsmaskoskärpa.

### `.Lg`

En stilklass som tillämpas på det inre innehållet, anger bakgrundsfärg, rundade hörn, skugga och vertikal layout och definierar layouten för de inre `h3` och `p` .