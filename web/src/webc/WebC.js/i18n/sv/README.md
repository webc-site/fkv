# Dokumentation och källkod för rendering av komponenter eller verktygsfunktioner

- Responsiv layout med två kolumner, med dokument som visas till vänster och källkoden visas till höger
- Dokumentation och källkodsstöd Redigera och Kod genvägsknappar för att hoppa till källadressen.
- Titeln och introduktionen visas överst och stöder automatisk översättning av i18n.

## Använd demon

```html
<c-webc-js src="/src/js/dom.js" href="/doc/{code}/js/dom.md"></c-webc-js>
```

## Egenskapsparametrar

- `src`: URL-adress för källkoden.
- `href`: URL-adress för dokumentmarkering, stöder användning av `{code}` platshållare för att anpassa till det aktuella språket.