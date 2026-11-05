# Erstellen Sie eine Popup-Ebene mit einer Schaltfläche zum Schließen und zeigen Sie sie an

- Funktionsaufruf zum Erstellen und Anzeigen einer Popup-Ebene
- Integrierte Schaltfläche zum Schließen. Klicken Sie, um das Element zu schließen und aus dem DOM zu entfernen
- Drücken Sie Esc, um das Eingabefeld zu defokussieren oder die Popup-Ebene zu schließen

## Nutzen Sie die Demo

```html
<button>Klicken Sie hier, um es aufzurufen</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "text";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## Schnittstellenparameter

Die Standard-Exportfunktion gibt `HTMLDialogElement` zurück und die Stilklassen `Box` und `X` wurden dem Dialogelement hinzugefügt.

## Stilklasse

### `.Box.X`

Wird auf das Element `dialog` angewendet und gibt den Dialogstil an.

### `a.X`

Wird auf das Element `a` angewendet und gibt den Stil der Schließen-Schaltfläche an.