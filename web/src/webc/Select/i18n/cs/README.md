# Rozbalovací výběr, šířka se přizpůsobí aktuální položce

Šířka nativního `select` je ve výchozím nastavení určena nejdelším textem volby. Při přepínání krátkých možností je příliš mnoho bílého prostoru, což ovlivňuje vzhled.

Tato komponenta transparentně překrývá nativní `select` nad tagem `b`, což umožňuje, aby se šířka přesně přizpůsobila délce aktuálně vybraného textu.

- Udržujte nativní `select` styly rozevírací nabídky a podporu usnadnění na všech terminálech
- Podporuje nativní interaktivní operace, jako je úplná navigace pomocí klávesnice a přepínání směrových kláves.
- Přizpůsobte šířku aktuální možnosti, abyste odstranili přebytečný bílý prostor

## Použijte demo

```html
<c-select>
  <option value="js">JavaScript</option>
  <option value="rs">Rust</option>
  <option value="py">Python</option>
</c-select>
```