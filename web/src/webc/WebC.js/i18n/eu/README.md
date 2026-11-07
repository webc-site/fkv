# Osagaiak edo tresna-funtzioak errendatzeko dokumentazioa eta iturburu-kodea

- Bi zutabe erantzuteko diseinua, dokumentuak ezkerrean bistaratzen dira eta iturburu-kodea eskuinean
- Dokumentazioa eta iturburu kodea onartzen Editatu eta Kode lasterbide botoiak iturburu helbidera salto egiteko.
- Izenburua eta sarrera goiko aldean bistaratzen dira, i18n itzulpen automatikoa onartzen duena.

## Erabili demoa

```html
<c-webc-js src="/src/js/dom.js" href="/doc/{code}/js/dom.md"></c-webc-js>
```

## Jabetza-parametroak

- `src`: iturburu-kodearen URL helbidea.
- `href`: dokumentuaren markatze URL helbidea, `{code}` leku-marka erabiltzea onartzen du uneko hizkuntzara egokitzeko.