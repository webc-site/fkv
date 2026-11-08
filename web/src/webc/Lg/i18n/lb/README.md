# Frostéiert Glas Hannergrond a Rand Highlight Effekt

Semi-transparent Grenz an banneschten Schiet kreéieren Mikro-Relief Rand Highlights.
Als Basis Stil Ofhängegkeet vun Knäppchen an Input Këscht Komponente.

## Benotzt d'Demo

```html
<main class="Lg">
  <article>
    <h3>Titel</h3>
    <pre>Inhalt Text</pre>
  </article>
</main>
```

## Stil Klass

### `.Lg`
Kaart Basis Stil. Fëllt `backdrop-filter` Onschärft, `border` Mikro-Embosséiert Grenz an `box-shadow` banneschten Schied un. Benotzt `svg/glass.svg#h` Filter op Hover.

### `main.Lg`
Den Haaptblockcontainer vun der Säit. Ofgerënnt Ecker `24px` mat reaktiounsfäeger Polsterung (`16px` wann d'Breet manner wéi `800px` ass, `24px` anescht).

### `.Btn`, `.BtnC`, `.Input`
Basis Komponent Klass Numm. Nodeems Dir dëse Stil agefouert hutt, kritt Dir automatesch flexibel Layout, hierarchesch Bezéiungen a Basiseigenschaften vum frostéierte Glas.