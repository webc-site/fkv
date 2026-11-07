# Dokumentation og kildekode til gengivelse af komponenter eller værktøjsfunktioner

- Responsivt layout med to kolonner, med dokumenter vist til venstre og kildekode vist til højre
- Understøttelse af dokumentation og kildekode Rediger og Kode genvejsknapper for at springe til kildeadressen.
- Titlen og introduktionen vises øverst og understøtter automatisk i18n-oversættelse.

## Brug demoen

```html
<c-webc-js src="/src/js/dom.js" href="/doc/{code}/js/dom.md"></c-webc-js>
```

## Ejendomsparametre

- `src`: URL-adresse for kildekoden.
- `href`: URL-adresse for dokumentmarkering, understøtter brug af `{code}` pladsholder til at tilpasse sig det aktuelle sprog.