# Typografistilar optimerade för läsupplevelse

- **Grundläggande typsättning**: Anpassa till rubriker, stycken, fetstil, kursiv stil, genomstruken, inline-kod, bilder och skiljelinjer.
- **Länkanimering**: Visa understruken animation när du håller muspekaren.
- **Projektlista**: Oordnade listor använder punkter, ordnade listor använder rundade serienummer i bakgrunden och stöder kapsling på flera nivåer.
- **Task List**: Ger kryssrutestil, stöder att göra och slutförd status.
- **Varningstips**: Stöder fem typer av varningsrutor: Notera, Tips, Viktigt, Varning och Varning.
- **Tabell och kod**: Ger markering av tabellrader och kodblock med skuggor och övertoningsbakgrunder.

## Använd demon

```html
<div class="Md">
  <h1>titel</h1>
  <p>Stycketext och <a href="#">länk</a></p>
  <hr>
  <h2>Andra nivån titel</h2>
  <ul><li><input type="checkbox" checked="" disabled=""> Uppgiftsobjekt</li></ul>
  <blockquote class="q note">
    <b class="qt"><i class="qi"></i>`<p>Anteckningsinnehåll</p>
  </blockquote>
</div>
```

## stilklass

Stilar tillämpas på HTML-element under `.Md`-behållaren.

### Varningsruta stil

Lägg till följande klassnamn på `blockquote`-elementet med `.q` klassnamnet för att växla varningsrutan:

- `note`: Anteckningar
- `tip`: Tips
- `important`: Viktigt
- `warning`: varning
- `caution`: Obs