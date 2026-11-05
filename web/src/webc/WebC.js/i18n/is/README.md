# Skjöl og frumkóði til að birta íhluti eða verkfæraaðgerðir

- Tveggja dálka móttækilegt skipulag, með skjölum sýnd til vinstri og frumkóði sýndur til hægri
- Stuðningur við skjöl og frumkóða Breyta og kóða flýtivísahnappa til að hoppa á upprunanetfangið.
- Titillinn og kynningin eru sýnd efst, sem styður i18n sjálfvirka þýðingu.

## Notaðu kynninguna

```html
<c-webc-js src="/src/js/dom.js" href="/doc/{code}/js/dom.md"></c-webc-js>
```

## Eignabreytur

- `src`: Vefslóð frumkóðans.
- `href`: Veffang skjalamerkingar, styður notkun `{code}` staðgengils til að laga sig að núverandi tungumáli.