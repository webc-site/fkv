# virtuális görgetősáv

- **Kétirányú görgetés**: Függőleges (`c-vs`) és vízszintes (`c-hs`) görgetést tesz lehetővé.
- **Méretre adott válasz**: Figyelje a tartalom és a terület méretének változásait, automatikusan frissíti a görgetősáv méretét és pozícióját, és automatikusan elrejti, ha nincs túlcsordulás.
- **Interaktív támogatás**: Támogatja a görgetést, a csúszka húzását és a pályakattintás pozicionálását.
- **Állapot elrejtése**: A csúszka megjelenítése és szélesítése lebegtetéskor vagy görgetéskor, és automatikusan elrejtése álló helyzetben.

## függőleges görgetés

```html
<div style="height:300px;border:1px solid #000">
  <c-vs>
    <b>1</b>
    <b>2</b>
    <b>3</b>
  </c-vs>
</div>
<style>b { display: flex; border: 2px solid #090; padding: 30px; margin: 30px; }</style>
```

## vízszintes görgetés

```html
<div style="width:300px;border:1px solid #000">
  <c-hs>
    <b>1</b>
    <b>2</b>
    <b>3</b>
  </c-hs>
</div>
<style>b { display: flex; border: 2px solid #090; margin: 30px; padding: 30px; }</style>
```

## stílusú részek

Testreszabhatja a Shadow DOM belső elemeinek stílusát a `::part()` segítségével:

- `::part(scroll)`: Görgetés a tartalomterületen.
- `::part(bar)`: Görgetősáv sávja.
- `::part(si)`: Görgetősáv csúszkája.
- `::part(i)`: Belső burkolóréteg.
- `::part(drag)`: A görgetősáv sávja húzott állapotban.

## stílus osztály

- `.drag`: Alkalmazva az `body` elemre, jelezve, hogy húzással görgetett állapotban van.

## Egyéni tulajdonságok

- `--si-bg`: A csúszka háttérszíne.
- `--si-anim`: Csúszka animáció.
- `--cursorScrollhSvg`: Az egérkurzor, amikor a vízszintes görgetősáv lebeg.
- `--cursorScrollvSvg`: Az egérkurzor, amikor a függőleges görgetősáv lebeg.