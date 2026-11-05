# Skep en vertoon opspringlaag

- Funksionele oproep om opspringlaag te skep en te vertoon
- Verwyder outomaties elemente uit die DOM wanneer dit gesluit is
- Voorkom Esc-sleutel kansellasiegedrag

## Gebruik die demo

```html
<button>Klik om te verskyn</button>

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
  description.textContent = "Teks";
  button.className = "Btn Main";
  button.textContent = "OK";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Interface parameters

### Verstek uitgevoer funksies

`Box()`

- **Terugwaarde**: `HTMLDialogElement`, die dialoogelement van `Box` stylklas is bygevoeg.

## styl klas

### `.Box`

Is van toepassing op die `dialog`-element om posisionering, sentrering en agtergrondmaskervervaging te stel.

### `.Lg`

'n Stylklas wat op die binne-inhoud toegepas word, wat die agtergrondkleur, afgeronde hoeke, skadu en vertikale uitleg stel, en die uitleg van die binneste `h3` en `p` definieer.