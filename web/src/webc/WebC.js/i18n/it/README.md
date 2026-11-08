# Documentazione e codice sorgente per il rendering di componenti o funzioni dello strumento

- Layout reattivo a due colonne, con i documenti visualizzati a sinistra e il codice sorgente visualizzato a destra
- La documentazione e il codice sorgente supportano i pulsanti di scelta rapida Modifica e codice per passare all'indirizzo di origine.
- Il titolo e l'introduzione vengono visualizzati in alto, supportando la traduzione automatica i18n.

## Utilizza la demo

```html
<c-webc-js src="/src/js/dom.js" href="/doc/{code}/js/dom.md"></c-webc-js>
```

## Parametri della proprietà

- `src`: indirizzo URL del codice sorgente.
- `href`: indirizzo URL del markdown del documento, supporta l'utilizzo del segnaposto `{code}` per adattarsi alla lingua corrente.