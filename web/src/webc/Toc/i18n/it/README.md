# Componente di navigazione delle directory

- **Cassetto reattivo**: visualizza la barra laterale sullo schermo ampio, comprimi automaticamente sullo schermo stretto e collabora con il cassetto `c-side` e il pulsante di attivazione del titolo
- **Driver multilingue**: supporta il dizionario degli oggetti o le funzioni multilingue, risponde automaticamente al cambio di lingua
- **Sottodirectory automatica**: analizza automaticamente i titoli `h1`~`h6` nel contenitore di destinazione per generare una directory secondaria e monitorare l'evidenziazione dello scorrimento della visualizzazione Hash.
- **Caricamento asincrono**: fai clic su un elemento della directory per ottenere automaticamente il testo Markdown in modo asincrono e inserirlo nell'area del contenuto, visualizzando `Ing` animazione di caricamento

## Utilizza la demo

```html
<c-toc></c-toc>
<c-md></c-md>

<script type="module">
import "webc.site/Toc.js";

const toc = document.querySelector("c-toc");
toc.target = document.querySelector("c-md");
toc.prefix = "doc";
toc.mdUrl = (key, lang) => "/doc/" + lang + "/" + key + ".md";
toc.li = (lang) => ({
  "README": "WebC.site",
  "usa": "usa componente"
});
</script>
```

## Parametri dell'interfaccia

### Proprietà e metodi

- `toc.prefix`: ottieni o imposta la stringa del prefisso di routing (come `"doc"`, la barra finale è facoltativa), la chiave `README` verrà automaticamente mappata su un percorso vuoto (ovvero il prefisso stesso)
- `toc.li`: Ottieni o imposta la funzione di generazione multilingue della directory `(lang) => ({ key: title })`
- `toc.mdUrl`: ottieni o imposta la funzione di generazione dell'URL Markdown `(key, lang) => url`
- `toc.target`: ottiene o imposta il nodo di rendering di destinazione associato (come `c-md`). Se non specificato, troverà automaticamente `c-md` / `main` / `article`
- `toc.load(key)`: attiva manualmente il caricamento del documento e l'espansione della directory corrispondente alla chiave
- `toc.sync()`: rianalizza manualmente il titolo nel nodo di destinazione corrente, sincronizza la directory secondaria ed evidenzia lo stato