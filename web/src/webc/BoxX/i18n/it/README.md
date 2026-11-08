# Crea e visualizza un livello popup con un pulsante di chiusura

- Chiamata funzionale per creare e visualizzare il livello popup
- Pulsante di chiusura integrato, fai clic per chiudere e rimuovere l'elemento dal DOM
- Premi Esc per sfocare la casella di input o chiudere il livello popup

## Utilizza la demo

```html
<button>Fare clic per apparire</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "Testo";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## Parametri dell'interfaccia

La funzione di esportazione predefinita restituisce `HTMLDialogElement` e le classi di stile `Box` e `X` sono state aggiunte all'elemento dialog.

## lezione di stile

### `.Box.X`

Applicato all'elemento `dialog`, specifica lo stile della finestra di dialogo.

### `a.X`

Applicato all'elemento `a`, specifica lo stile del pulsante di chiusura.