# Erstellt a weist Popup Layer

- Funktionell Uruff fir Popupschicht ze kreéieren an ze weisen
- Ewechzehuelen automatesch Elementer aus der DOM wann zougemaach
- Verhënneren Esc Schlëssel Annulatioun Verhalen

## Benotzt d'Demo

```html
<button>Klickt fir opzemaachen</button>

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

## Interface Parameteren

### Standard exportéiert Funktiounen

`Box()`

- **Retourwäert**: `HTMLDialogElement`, den Dialogelement vun der `Box` Stilklass gouf dobäigesat.

## Stil Klass

### `.Box`

Gëllt fir d'`dialog` Element fir d'Positionéierung, d'Zentraléierung an d'Backgroundmasken ze setzen.

### `.Lg`

Eng Stilklass applizéiert op den bannenzegen Inhalt, setzt d'Hannergrondfaarf, gerundeten Ecker, Schatten a vertikalen Layout, an definéiert de Layout vum banneschten `h3` an `p` .