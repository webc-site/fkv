# Portale di routing che ospita la documentazione della libreria dei componenti e un sito demo

- **Layout a schermo intero**: il posizionamento assoluto riempie il viewport, con scorrimento e navigazione integrati.
- **Routing dinamico**: analizza i percorsi URL e carica dinamicamente demo o home page dei componenti.
- **Multilingue**: rispondi e aggiorna la barra di navigazione e il testo della pagina in tempo reale.
- **Cambio di stato**: mantiene automaticamente le classi di stile di caricamento e home page.

## Utilizza la demo

```html
<c-webc-site></c-webc-site>
```

## lezione di stile

Il componente cambierà al suo interno i seguenti nomi di classe:

- `Ing`: caricamento del percorso o analisi del pacchetto lingua.
- `A`: attualmente sulla home page.