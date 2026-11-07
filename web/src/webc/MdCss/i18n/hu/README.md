# Az olvasási élményhez optimalizált tipográfiai stílusok

- **Alapszedés**: alkalmazkodik a címekhez, bekezdésekhez, félkövér, dőlt, áthúzott, soron belüli kód, képek és elválasztó vonalak.
- **Link animáció**: Aláhúzott animáció megjelenítése, amikor lebeg.
- **Projektlista**: A rendezetlen listák felsorolásjeleket, a rendezett listák lekerekített háttér sorozatszámokat használnak, és támogatják a többszintű egymásba ágyazást.
- **Feladatlista**: jelölőnégyzet stílust biztosít, támogatja a teendőket és a befejezett állapotot.
- **Figyelmeztető tippek**: Öt típusú figyelmeztető dobozt támogat: Megjegyzés, Tipp, Fontos, Figyelmeztetés és Figyelem.
- **Táblázatok és kód**: Táblázatsorok kiemelését és kódblokkokat biztosít árnyékokkal és színátmenetes hátterekkel.

## Használd a demót

```html
<div class="Md">
  <h1>cím</h1>
  <p>Bekezdés szövege és <a href="#">link</a></p>
  <hr>
  <h2>Második szintű cím</h2>
  <ul><li><input type="checkbox" checked="" disabled=""> Feladatelem</li></ul>
  <blockquote class="q note">
    <b class="qt"><i class="qi"></i>`<p>Jegyzettartalom</p>
  </blockquote>
</div>
```

## stílus osztály

A stílusok a `.Md` tároló alatti HTML-elemekre vonatkoznak.

### Figyelmeztető doboz stílusa

Adja hozzá a következő osztálynevet az `blockquote` elemhez az `.q` osztálynévvel a figyelmeztető mező váltásához:

- `note`: Megjegyzések
- `tip`: Tipp
- `important`: Fontos
- `warning`: figyelmeztetés
- `caution`: Megjegyzés