# virtuel rullepanel

- **Tovejs rulning**: Giver lodret (`c-vs`) og vandret (`c-hs`) rulning.
- **Størrelsesrespons**: Overvåg indhold og områdestørrelsesændringer, opdater automatisk rullepanelets størrelse og position, og skjul automatisk, når der ikke er overløb.
- **Interaktiv support**: Understøtter hjulrulning, skydertræk og sporklikpositionering.
- **Statusskjul**: Vis og udvid skyderen, når du svæver eller ruller, og skjul den automatisk, når den står stille.

## lodret rulle

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

## vandret rulle

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

## stildele

Tilpas stilen for Shadow DOM interne elementer via `::part()`:

- `::part(scroll)`: Rul gennem indholdsområdet.
- `::part(bar)`: Spor med rullepanel.
- `::part(si)`: Skyder til rullepanel.
- `::part(i)`: Internt indpakningslag.
- `::part(drag)`: Rullebjælkens spor i træktilstand.

## stil klasse

- `.drag`: Anvendt på `body` elementet, hvilket indikerer, at det er i en træk-scroll-tilstand.

## Brugerdefinerede egenskaber

- `--si-bg`: Skyderens baggrundsfarve.
- `--si-anim`: Skyderanimation.
- `--cursorScrollhSvg`: Musemarkøren, når den vandrette rullebjælke holdes.
- `--cursorScrollvSvg`: Musemarkøren, når den lodrette rullebjælke holdes.