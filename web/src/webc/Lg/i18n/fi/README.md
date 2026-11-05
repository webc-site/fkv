# Himmeä lasi tausta ja reunan korostustehoste

Puoliläpinäkyvä reunus ja sisävarjo luovat mikroreljeefreunojen kohokohtia.
Painikkeiden ja syöttölaatikoiden komponenttien perustyyliriippuvuutena.

## Käytä demoa

```html
<main class="Lg">
  <article>
    <h3>otsikko</h3>
    <pre>Sisältöteksti</pre>
  </article>
</main>
```

## tyyliluokka

### `.Lg`
Kortin perustyyli. Käytä `backdrop-filter` sumennusta, `border` mikrokohokuvioitua reunaa ja `box-shadow` sisävarjoa. Käytä `svg/glass.svg#h` -suodatinta viemällä hiiri.

### `main.Lg`
Sivun päälohkosäilö. Pyöristetyt kulmat `24px` responsiivisella pehmusteella (`16px` jos leveys on pienempi kuin `800px`, `24px` muuten).

### `.Btn`, `.BtnC`, `.Input`
Peruskomponenttiluokan nimi. Tämän tyylin käyttöönoton jälkeen saat automaattisesti joustavan ulkoasun, hierarkkiset suhteet ja himmeän lasin perusominaisuudet.