# Pozadie z matného skla a efekt zvýraznenia okrajov

Polopriehľadný okraj a vnútorný tieň vytvárajú mikroreliéfne zvýraznenia okrajov.
Ako základná štýlová závislosť komponentov tlačidla a vstupného boxu.

## Použite demo

```html
<main class="Lg">
  <article>
    <h3>titul</h3>
    <pre>Obsahový text</pre>
  </article>
</main>
```

## štýlová trieda

### `.Lg`
Základný štýl karty. Použiť `backdrop-filter` rozmazanie, `border` mikroreliéfne orámovanie a `box-shadow` vnútorný tieň. Použiť filter `svg/glass.svg#h` pri umiestnení kurzora myši.

### `main.Lg`
Hlavný blokový kontajner stránky. Zaoblené rohy `24px` s citlivým čalúnením (`16px` ak je šírka menšia ako `800px`, inak `24px`).

### `.Btn`, `.BtnC`, `.Input`
Názov triedy základného komponentu. Po zavedení tohto štýlu automaticky získate flexibilné usporiadanie, hierarchické vzťahy a základné vlastnosti matného skla.