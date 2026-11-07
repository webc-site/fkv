# Mattierter Glashintergrund und Rand-Highlight-Effekt

Der halbtransparente Rand und der Innenschatten erzeugen Mikrorelief-Kanten-Highlights.
Als grundlegende Stilabhängigkeit von Schaltflächen- und Eingabefeldkomponenten.

## Nutzen Sie die Demo

```html
<main class="Lg">
  <article>
    <h3>Titel</h3>
    <pre>Inhaltstext</pre>
  </article>
</main>
```

## Stilklasse

### `.Lg`
Kartengrundstil. Tragen Sie `backdrop-filter` Unschärfe, `border` mikrogeprägten Rand und `box-shadow` Innenschatten auf. Wenden Sie den `svg/glass.svg#h`-Filter beim Hover an.

### `main.Lg`
Der Hauptblockcontainer der Seite. Abgerundete Ecken `24px` mit reaktionsfähiger Polsterung (`16px` wenn die Breite kleiner als `800px` ist, andernfalls `24px`).

### `.Btn`, `.BtnC`, `.Input`
Name der Basiskomponentenklasse. Nach der Einführung dieses Stils erhalten Sie automatisch ein flexibles Layout, hierarchische Beziehungen und grundlegende Eigenschaften von Milchglas.