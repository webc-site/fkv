# Dokumentace a zdrojový kód pro renderování komponent nebo funkcí nástroje

- Responzivní rozvržení se dvěma sloupci s dokumenty zobrazenými vlevo a zdrojovým kódem zobrazeným vpravo
- Dokumentace a zdrojový kód podporují tlačítka Upravit a Kód pro přechod na zdrojovou adresu.
- Nadpis a úvod jsou zobrazeny nahoře a podporují automatický překlad i18n.

## Použijte demo

```html
<c-webc-js src="/src/js/dom.js" href="/doc/{code}/js/dom.md"></c-webc-js>
```

## Parametry vlastnosti

- `src`: URL adresa zdrojového kódu.
- `href`: URL adresa označení dokumentu, podporuje použití zástupného symbolu `{code}` pro přizpůsobení aktuálnímu jazyku.