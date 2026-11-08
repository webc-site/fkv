# Búðu til og sýndu sprettigluggalag

- Virkt símtal til að búa til og sýna sprettigluggalag
- Fjarlægðu sjálfkrafa þætti úr DOM þegar þeir eru lokaðir
- Koma í veg fyrir hegðun Esc-lykils

## Notaðu kynninguna

```html
<button>Smelltu til að poppa upp</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "Titill";
  description.textContent = "Texti";
  button.className = "Btn Main";
  button.textContent = "Í lagi";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Viðmótsbreytur

### Sjálfgefnar útfluttar aðgerðir

`Box()`

- **Return value**: `HTMLDialogElement`, gluggaeiningunni í `Box` stílflokknum hefur verið bætt við.

## stílaflokkur

### `.Box`

Á við um `dialog` þáttinn til að stilla staðsetningu, miðju og óskýrleika bakgrunnsgrímu.

### `.Lg`

Stílflokkur notaður á innra innihaldið, stillir bakgrunnslit, ávöl horn, skugga og lóðrétt útlit og skilgreinir útlit innra `h3` og `p` .