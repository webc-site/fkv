# Popup-Ebene erstellen und anzeigen

- Funktionsaufruf zum Erstellen und Anzeigen einer Popup-Ebene
- Elemente beim Schließen automatisch aus dem DOM entfernen
- Verhindern Sie das Abbruchverhalten der Esc-Taste

## Nutzen Sie die Demo

```html
<button>Klicken Sie hier, um es aufzurufen</button>

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

## Schnittstellenparameter

### Standardmäßig exportierte Funktionen

`Box()`

- **Rückgabewert**: `HTMLDialogElement`, das Dialogelement der Stilklasse `Box` wurde hinzugefügt.

## Stilklasse

### `.Box`

Gilt für das Element `dialog`, um Positionierung, Zentrierung und Unschärfe der Hintergrundmaske festzulegen.

### `.Lg`

Eine Stilklasse, die auf den inneren Inhalt angewendet wird, die Hintergrundfarbe, abgerundete Ecken, Schatten und vertikales Layout festlegt und das Layout der inneren `h3` und `p` definiert.