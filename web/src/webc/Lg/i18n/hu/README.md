# Tetüveg háttér és élkiemelés hatása

A félig átlátszó szegély és a belső árnyék mikrodomborműves élek kiemelését eredményezi.
A gomb és a beviteli doboz összetevőinek alapvető stílusfüggőségeként.

## Használd a demót

```html
<main class="Lg">
  <article>
    <h3>cím</h3>
    <pre>Tartalmi szöveg</pre>
  </article>
</main>
```

## stílus osztály

### `.Lg`
Kártya alapstílus. Alkalmazzon `backdrop-filter` életlenítést, `border` mikrodombornyomott szegélyt és `box-shadow` belső árnyékot. Alkalmazza az `svg/glass.svg#h` szűrőt az egérmutatóval.

### `main.Lg`
Az oldal fő blokktárolója. Lekerekített sarkok `24px` reszponzív kitöltéssel (`16px` ha a szélesség kisebb, mint `800px`, egyébként `24px`).

### `.Btn`, `.BtnC`, `.Input`
Alapkomponens osztálynév. A stílus bevezetése után automatikusan megkapja a mattüveg rugalmas elrendezését, hierarchikus kapcsolatait és alapvető tulajdonságait.