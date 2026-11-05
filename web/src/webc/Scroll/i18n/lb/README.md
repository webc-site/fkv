# virtuell Scrollbar

- **Zwee-Wee Scrolling**: Bitt vertikal (`c-vs`) an horizontal (`c-hs`) Scrolling.
- ** Gréisst Äntwert **: Monitor Inhalt a Beräich Gréisst Ännerungen, update automatesch d'Scrollbar Gréisst a Positioun, a verstoppt automatesch wann et keen Iwwerfloss ass.
- ** Interaktiv Ënnerstëtzung **: Ënnerstëtzt Rad Scrollen, Schieber zéien an Track Klick Positionéierung.
- ** Status Hide **: Show a vergréissert de Schieber wann Dir schwieft oder scrollt, a verstoppt se automatesch wann se stationär sinn.

## vertikal Scroll

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

## horizontal Scroll

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

## Stil Deeler

Personnaliséiert de Stil vun Shadow DOM intern Elementer iwwer `::part()`:

- `::part(scroll)`: Scroll den Inhaltsberäich.
- `::part(bar)`: Scrollbar Track.
- `::part(si)`: Scrollbar Schieber.
- `::part(i)`: Intern Wrapping Layer.
- `::part(drag)`: D'Scrollbar-Streck am Zuchzoustand.

## Stil Klass

- `.drag`: Op den `body` Element ugewannt, wat beweist datt et an engem Drag-Scrolling-Staat ass.

## Benotzerdefinéiert Eegeschafte

- `--si-bg`: Schieber Hannergrondfaarf.
- `--si-anim`: Slider Animatioun.
- `--cursorScrollhSvg`: De Mauszeiger wann d'horizontale Scrollbar gedréckt ass.
- `--cursorScrollvSvg`: De Mauszeiger wann d'vertikal Scrollbar gedréckt ass.