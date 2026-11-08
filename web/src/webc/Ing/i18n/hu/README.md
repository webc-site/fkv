# Betöltési vagy feldolgozási állapot jelzője

- Egyesítse a kurzort várakozási állapotba
- Hullámos animációt jelenít meg a feldolgozás állapotának jelzésére
- Alkalmazzon elhomályosító maszkot az űrlapra az ismétlődő beküldések elkerülése érdekében

## Használd a demót

```html
<!-- független terhelésjelző -->
<div class="Ing"></div>

<!-- Űrlap betöltése maszkkal -->
<form class="Ing">
  <input type="text" placeholder="Beviteli doboz">
  <button type="submit">benyújtani</button>
</form>
```

## stílus osztály

### `.Ing`
- A mutató állapota `wait`.
- A `::before` pszeudoelem hullámos animációt jelenít meg.
- Ha az `form` elemen használják, az `::after` pszeudoelem 1px-es elmosódási maszkot jelenít meg, letiltva az interakciót és a szövegkijelölést.