# Különböző képernyőméretekhez igazított navigációs menü

- Asztal: A hivatkozások vízszintes csempékben jelennek meg
- Mobil verzió: csukja be a menügombba, kattintson az oldalsáv fiókjának kihúzásához
- Útvonal aktiválása: automatikusan hasonlítsa össze az aktuális elérési utat és a hash értéket, és adja hozzá az aktiválási állapot osztályának nevét a megfelelő linkhez `A`
- Automatikus bezárás: Amikor a mobilterminál egy hivatkozásra vagy az oldalsáv külső hátterére kattint, az oldalsáv automatikusan bezárul.

## használat

```html
<c-nav-menu>
  <a href="/">címlapon</a>
  <a href="/doc">dokumentum</a>
  <a href="/about">körülbelül</a>
</c-nav-menu>
```

## szemléltet

Az összetevő inicializálásakor, ha nincs `c-side` a gyermekelemben, a mobil menügomb és az `c-side` oldalsáv automatikusan létrejön, és az összes `a` címke átkerül az oldalsávba.

## stílus osztály

- **A**: Az aktivált link automatikusan hozzáadja az osztálynevet `A`