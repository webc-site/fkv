# Dokumentacija ir šaltinio kodas komponentams arba įrankių funkcijoms pateikti

- Dviejų stulpelių interaktyvus išdėstymas, kai dokumentai rodomi kairėje, o šaltinio kodas rodomas dešinėje
- Dokumentacija ir šaltinio kodas palaiko sparčiuosius mygtukus Redaguoti ir Kodas, kad pereitumėte į šaltinio adresą.
- Pavadinimas ir įvadas rodomi viršuje, palaiko i18n automatinį vertimą.

## Naudokite demonstracinę versiją

```html
<c-webc-js src="/src/js/dom.js" href="/doc/{code}/js/dom.md"></c-webc-js>
```

## Savybės parametrai

- `src`: šaltinio kodo URL adresas.
- `href`: dokumento žymėjimo URL adresas, palaikomas `{code}` rezervuotos vietos naudojimas, norint prisitaikyti prie esamos kalbos.