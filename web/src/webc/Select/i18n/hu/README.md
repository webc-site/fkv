# Legördülő menü, a szélesség az aktuális tételhez igazodik

A natív `select` szélességét alapértelmezés szerint a leghosszabb opciószöveg határozza meg. A rövid opciók váltásakor túl sok a fehér hely, ami befolyásolja a megjelenést.

Ez az összetevő átlátszóan lefedi a natív `select` `b` címke felett, lehetővé téve, hogy a szélesség pontosan alkalmazkodjon az aktuálisan kijelölt szöveg hosszához.

- Fenntartja a natív `select` legördülő menüstílusokat és a kisegítő lehetőségek támogatását minden terminálon
- Támogatja a natív interaktív műveleteket, például a teljes billentyűzet Tab navigációt és az iránybillentyűk váltását.
- Állítsa a szélességet az aktuális beállításhoz, hogy kiküszöbölje a felesleges fehér teret

## Használd a demót

```html
<c-select>
  <option value="js">JavaScript</option>
  <option value="rs">Rust</option>
  <option value="py">Python</option>
</c-select>
```