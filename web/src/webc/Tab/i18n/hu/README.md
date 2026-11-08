# Váltható lapok

- A címke kiválasztott állapota zökkenőmentesen integrálva van az alábbi tartalomterülettel.
- Szerkezeti specifikációk: Az alárendelt elemek a `nav > a` címke-navigációra vannak rögzítve, a közvetlen `b` gyermekelem pedig tartalomterületként.
- Automatikusan társítsa a `a[value]` címkét a `b[slot]` tartalomcsomóponthoz.
- Támogatja az aktuális aktív oldal automatikus megőrzését a `localStorage`-ban az `key` attribútumon keresztül.
- Támogatja az aktivált lapoldal megadását a `value` attribútummal (ha nincs gyorsítótár és `value`, akkor alapértelmezés szerint a `class="A"` vagy az első címke kerül kiválasztásra).
- Könnyű Könnyű DOM implementáció, nincs Shadow DOM akadály, nagyfokú szabadság a stílus testreszabásában.

## Használd a demót

```html
<c-tab key="my-tab-key" value="banana">
  <nav>
    <a value="apple">alma</a>
    <a value="banana">banán</a>
    <a value="orange">mandarin</a>
  </nav>
  <b slot="apple">Az alma tápláló gyümölcs</b>
  <b slot="banana">A banán trópusi gyümölcs</b>
  <b slot="orange">A narancs C-vitaminban gazdag</b>
</c-tab>
```

## ingatlan

- `key`: a localStorage fennmaradásához használt kulcsnév
- `value`: jelenleg kiválasztott érték

## Egyedi események

- `change`: A lapváltáskor aktiválódik, az `e.value` az aktuálisan aktivált címke `value` értéke.