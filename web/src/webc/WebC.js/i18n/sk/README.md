# Dokumentácia a zdrojový kód pre vykresľovanie komponentov alebo funkcií nástroja

- Responzívne rozloženie v dvoch stĺpcoch s dokumentmi zobrazenými naľavo a zdrojovým kódom zobrazeným napravo
- Dokumentácia a zdrojový kód podporujú tlačidlá Upraviť a Kód na prechod na zdrojovú adresu.
- Nadpis a úvod sa zobrazujú v hornej časti a podporujú automatický preklad i18n.

## Použite demo

```html
<c-webc-js src="/src/js/dom.js" href="/doc/{code}/js/dom.md"></c-webc-js>
```

## Parametre vlastnosti

- `src`: URL adresa zdrojového kódu.
- `href`: URL adresa označenia dokumentu, podporuje použitie `{code}` zástupného symbolu na prispôsobenie sa aktuálnemu jazyku.