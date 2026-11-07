# Dokumentáció és forráskód a komponensek vagy szerszámfunkciók megjelenítéséhez

- Kétoszlopos reszponzív elrendezés, a dokumentumok bal oldalon, a forráskód pedig a jobb oldalon
- A dokumentáció és a forráskód támogatja a Szerkesztés és a Kód gyorsbillentyűket a forráscímre ugráshoz.
- A cím és a bevezető felül jelenik meg, támogatva az i18n automatikus fordítását.

## Használd a demót

```html
<c-webc-js src="/src/js/dom.js" href="/doc/{code}/js/dom.md"></c-webc-js>
```

## Tulajdonság paraméterei

- `src`: a forráskód URL-címe.
- `href`: a dokumentum jelölésének URL-címe, támogatja a `{code}` helyőrző használatát az aktuális nyelvhez való alkalmazkodáshoz.