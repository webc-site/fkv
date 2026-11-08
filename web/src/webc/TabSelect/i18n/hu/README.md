# Lapoldal legördülő menüvel

- Támogatja az adaptív szélesség legördülő választódobozának (`<c-select>`) beágyazását a címkébe, amely csak akkor jelenik meg, ha az aktuális lapoldal aktiválva van.
- Szerkezeti specifikáció: Az alárendelt elem a `nav > a` elemet használja címke navigációként, és a közvetlen gyermekelem `b` tartalomterületként szolgál.
- Automatikusan társítja a `a[value]` címkét, az `a` a `c-select` címkét (a csoport értékattribútuma és a kiválasztott beállítás értéke alapján) és a `b[slot]` tartalomcsomópontot.
- Támogatja az aktuálisan aktivált elem automatikus megőrzését a `localStorage` számára a `key` attribútumon keresztül.
- Támogatja az egyes csoportok korábbi részkijelöléseinek automatikus mentését és visszaállítását `localStorage` a `key + '/' + tab` használatával.
- Támogatja az aktív lap megadását az `value` attribútummal.

## Használd a demót

```html
<c-tab-select key="my-tab-select-key" value="language/js">
  <nav>
    <a value="language">
      <span>nyelv</span>
      <c-select>
        <option value="js">JavaScript</option>
        <option value="rs">Rust</option>
      </c-select>
    </a>
    <a value="os">
      <span>rendszer</span>
      <c-select>
        <option value="mac">macOS</option>
        <option value="win">Windows</option>
      </c-select>
    </a>
  </nav>
  <b slot="language">
    <b slot="js">A JavaScript egy szkriptnyelv</b>
    <b slot="rs">A Rust egy rendszerszintű programozási nyelv</b>
  </b>
  <b slot="os">
    <b slot="mac">A macOS egy Unix-alapú grafikus operációs rendszer</b>
    <b slot="win">A Windows a Microsoft által fejlesztett operációs rendszer</b>
  </b>
</c-tab-select>
```

## ingatlan

- `key`: a localStorage fennmaradásához használt kulcsnév
- `value`: jelenleg kiválasztott érték

## Egyedi események

- `change`: A lapok közötti váltáskor aktiválódik, az `e.value` az aktuálisan aktivált opció `value` értéke.