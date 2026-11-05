# A navigációs menü és a visszatérés gomb automatikus váltása

- A gyermekelemek automatikus tördelése a navigációs menü és a visszatérési gomb szerkezeteinek létrehozásához
- Az útvonalak előzményeinek automatikus nyomon követése és az előző visszatérési útvonal kiszámítása
- Jelenítse meg a navigációs menüt, ha az útvonal megegyezik a menüponttal, ellenkező esetben jelenítse meg a vissza gombot

## Használd a demót

```html
<c-nav-l>
  <a href="/">címlapon</a>
  <a href="/doc">dokumentum</a>
</c-nav-l>
```

## stílus osztály

- `B`: aktiválja a visszatérés gomb állapotát. A vissza gomb megjelenítése és a navigációs menü elrejtése. Az útvonalválasztás alapján automatikusan átkapcsol az összetevőn belül.