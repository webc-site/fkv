# Fundal din sticlă mată și efect de evidențiere a marginilor

Chenarul semi-transparent și umbra interioară creează evidențieri de margine în microrelief.
Ca dependență de stil de bază a componentelor butoanelor și casetei de intrare.

## Utilizați demonstrația

```html
<main class="Lg">
  <article>
    <h3>titlu</h3>
    <pre>Text de conținut</pre>
  </article>
</main>
```

## clasa de stil

### `.Lg`
Stil de bază card. Aplicați `backdrop-filter` estompare, `border` margine micro-embosată și `box-shadow` umbra interioară. Aplicați filtrul `svg/glass.svg#h` la trecerea cursorului.

### `main.Lg`
Containerul bloc principal al paginii. Colțuri rotunjite `24px` cu umplutură receptivă (`16px` dacă lățimea este mai mică de `800px`, `24px` în caz contrar).

### `.Btn`, `.BtnC`, `.Input`
Numele clasei componente de bază. După introducerea acestui stil, veți obține automat un aspect flexibil, relații ierarhice și proprietăți de bază ale sticlei mate.