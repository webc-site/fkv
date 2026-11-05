# Crea e visualizza il livello popup

- Chiamata funzionale per creare e visualizzare il livello popup
- Rimuovi automaticamente gli elementi dal DOM quando viene chiuso
- Impedisce il comportamento di annullamento del tasto Esc

## Utilizza la demo

```html
<button>Fare clic per apparire</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  titolo.textContent = "Titolo";
  description.textContent = "Testo";
  button.className = "Btn Main";
  button.textContent = "OK";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Parametri dell'interfaccia

### Funzioni esportate predefinite

`Box()`

- **Valore restituito**: `HTMLDialogElement`, è stato aggiunto l'elemento dialog della classe di stile `Box`.

## lezione di stile

### `.Box`

Si applica all'elemento `dialog` per impostare il posizionamento, la centratura e la sfocatura della maschera dello sfondo.

### `.Lg`

Una classe di stile applicata al contenuto interno, impostando il colore di sfondo, gli angoli arrotondati, l'ombra e il layout verticale e definendo il layout dei contenuti interni `h3` e `p` .