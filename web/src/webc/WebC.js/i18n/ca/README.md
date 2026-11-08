# Documentació i codi font per a la representació de components o funcions d'eina

- Disseny responsiu de dues columnes, amb els documents que es mostren a l'esquerra i el codi font a la dreta
- La documentació i el codi font són compatibles amb els botons d'edició i de drecera de codi per saltar a l'adreça font.
- El títol i la introducció es mostren a la part superior i admeten la traducció automàtica i18n.

## Utilitzeu la demostració

```html
<c-webc-js src="/src/js/dom.js" href="/doc/{code}/js/dom.md"></c-webc-js>
```

## Paràmetres de propietat

- `src`: adreça URL del codi font.
- `href`: adreça URL de la reducció del document, admet l'ús de `{code}` marcador de posició per adaptar-se a l'idioma actual.