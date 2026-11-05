# Indikátor stavu nakládky nebo zpracování

- Sjednoťte kurzor do stavu čekání
- Zobrazuje animaci zvlnění indikující stav zpracování
- Použijte masku rozostření na formulář, abyste zabránili duplicitnímu odeslání

## Použijte demo

```html
<!-- nezávislý indikátor zatížení -->
<div class="Ing"></div>

<!-- Načítání formuláře s maskou -->
<form class="Ing">
  <input type="text" placeholder="Vstupní pole">
  <button type="submit">předložit</button>
</form>
```

## stylová třída

### `.Ing`
- Stav ukazatele je `wait`.
- Pseudoelement `::before` vykreslí animaci vlnění.
- Při použití na prvku `form` pseudoprvek `::after` vykreslí masku rozostření o velikosti 1 pixel, čímž zakáže interakci a výběr textu.