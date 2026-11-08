# Crear i mostrar una capa emergent

- Crida funcional per crear i mostrar la capa emergent
- Elimina automàticament elements del DOM quan està tancat
- Evitar el comportament de cancel·lació de la tecla Esc

## Utilitzeu la demostració

```html
<button>Feu clic per aparèixer</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "Títol";
  description.textContent = "Text";
  button.className = "Btn Main";
  button.textContent = "D'acord";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Paràmetres de la interfície

### Funcions exportades per defecte

`Box()`

- **Valor de retorn**: `HTMLDialogElement`, s'ha afegit l'element de diàleg de la classe d'estil `Box`.

## classe d'estil

### `.Box`

S'aplica a l'element `dialog` per definir el posicionament, el centratge i el desenfocament de la màscara de fons.

### `.Lg`

Una classe d'estil aplicada al contingut interior, establint el color de fons, les cantonades arrodonides, l'ombra i la disposició vertical, i definint la disposició dels `h3` i `p` interiors.