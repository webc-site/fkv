# Pozadí z matného skla a efekt zvýraznění okrajů

Poloprůhledný okraj a vnitřní stín vytvářejí mikroreliéfní zvýraznění okrajů.
Jako základní stylová závislost komponent tlačítka a vstupního pole.

## Použijte demo

```html
<main class="Lg">
  <article>
    <h3>titul</h3>
    <pre>Text obsahu</pre>
  </article>
</main>
```

## stylová třída

### `.Lg`
Základní styl karty. Použijte `backdrop-filter` rozostření, `border` mikroembosovaný okraj a `box-shadow` vnitřní stín. Při umístění kurzoru použijte filtr `svg/glass.svg#h`.

### `main.Lg`
Hlavní blokový kontejner stránky. Zaoblené rohy `24px` s citlivým polstrováním (`16px` pokud je šířka menší než `800px`, jinak `24px`).

### `.Btn`, `.BtnC`, `.Input`
Název třídy základní komponenty. Po zavedení tohoto stylu automaticky získáte flexibilní uspořádání, hierarchické vztahy a základní vlastnosti matného skla.