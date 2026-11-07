# virtuell rullningslist

- **Tvåvägsrullning**: Ger vertikal (`c-vs`) och horisontell (`c-hs`) rullning.
- **Storleksrespons**: Övervaka ändringar av innehåll och områdesstorlek, uppdatera automatiskt rullningslistens storlek och position och dölj automatiskt när det inte finns något överflöd.
- **Interaktivt stöd**: Stöder hjulrullning, skjutreglage och positionering av spårklick.
- **Statusdölj**: Visa och bredda skjutreglaget när du håller muspekaren eller rullar, och dölj det automatiskt när det står stilla.

## vertikal rullning

```html
<div style="height:300px;border:1px solid #000">
  <c-vs>
    <b>1</b>
    <b>2</b>
    <b>3</b>
  </c-vs>
</div>
<style>b { display: flex; border: 2px solid #090; padding: 30px; margin: 30px; }</style>
```

## horisontell rullning

```html
<div style="width:300px;border:1px solid #000">
  <c-hs>
    <b>1</b>
    <b>2</b>
    <b>3</b>
  </c-hs>
</div>
<style>b { display: flex; border: 2px solid #090; margin: 30px; padding: 30px; }</style>
```

## stildelar

Anpassa stilen för Shadow DOM interna element via `::part()`:

- `::part(scroll)`: Bläddra i innehållsområdet.
- `::part(bar)`: Spår med rullningslist.
- `::part(si)`: Rullningsreglage.
- `::part(i)`: Internt omslagsskikt.
- `::part(drag)`: Rullningslistens spår i dragläge.

## stilklass

- `.drag`: Tillämpas på `body`-elementet, vilket indikerar att det är i ett drag-rullningsläge.

## Anpassade egenskaper

- `--si-bg`: Reglagets bakgrundsfärg.
- `--si-anim`: Reglageanimering.
- `--cursorScrollhSvg`: Muspekaren när den horisontella rullningslisten förs över.
- `--cursorScrollvSvg`: Muspekaren när den vertikala rullningslisten förs över.