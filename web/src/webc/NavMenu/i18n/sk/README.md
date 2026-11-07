# Navigačné menu prispôsobené rôznym veľkostiam obrazovky

- Pracovná plocha: Odkazy sa zobrazujú vo vodorovných dlaždiciach
- Mobilná verzia: Zbaliť do tlačidla ponuky, kliknutím vysunúť zásuvku bočného panela
- Aktivácia trasy: automaticky porovnajte aktuálnu cestu a hodnotu hash a pridajte názov triedy stavu aktivácie k zodpovedajúcemu odkazu `A`
- Automaticky zatvoriť: Keď mobilný terminál klikne na odkaz alebo externé pozadie bočného panela, bočný panel sa automaticky zatvorí.

## použitie

```html
<c-nav-menu>
  <a href="/">titulná strana</a>
  <a href="/doc">dokument</a>
  <a href="/about">o</a>
</c-nav-menu>
```

## ilustrovať

Keď je komponent inicializovaný, ak v podradenom prvku nie je `c-side`, automaticky sa vytvorí tlačidlo ponuky pre mobil a bočný panel `c-side` a všetky značky `a` sa presunú do bočného panela.

## štýlová trieda

- **A**: Aktivovaný odkaz automaticky pridá názov triedy `A`