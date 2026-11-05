# Dokumentasjon og kildekode for gjengivelse av komponenter eller verktøyfunksjoner

- Responsiv layout med to kolonner, med dokumenter vist til venstre og kildekode vist til høyre
- Dokumentasjon og kildekodestøtte Rediger og Kode snarveisknapper for å hoppe til kildeadressen.
- Tittelen og introduksjonen vises øverst, og støtter i18n automatisk oversettelse.

## Bruk demoen

```html
<c-webc-js src="/src/js/dom.js" href="/doc/{code}/js/dom.md"></c-webc-js>
```

## Egenskapsparametere

- `src`: URL-adressen til kildekoden.
- `href`: URL-adresse til dokumentmarkering, støtter bruk av `{code}` plassholder for å tilpasse seg gjeldende språk.