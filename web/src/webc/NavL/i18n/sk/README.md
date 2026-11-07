# Automaticky prepína navigačné menu a tlačidlo návratu

- Automaticky zalamovať podradené prvky na generovanie štruktúr navigačnej ponuky a tlačidiel návratu
- Automaticky sledujte históriu smerovania a vypočítajte predchádzajúcu spiatočnú cestu
- Zobrazte navigačnú ponuku, keď sa trasa zhoduje s položkou ponuky, inak zobrazte tlačidlo späť

## Použite demo

```html
<c-nav-l>
  <a href="/">titulná strana</a>
  <a href="/doc">dokument</a>
</c-nav-l>
```

## štýlová trieda

- `B`: aktivuje stav tlačidla návratu. Zobraziť tlačidlo späť a skryť navigačnú ponuku. Automaticky prepínané v rámci komponentu na základe smerovania.