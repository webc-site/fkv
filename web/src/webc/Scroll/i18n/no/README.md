# virtuell rullefelt

- **Toveis rulling**: Gir vertikal (`c-vs`) og horisontal (`c-hs`) rulling.
- **Størrelsesrespons**: Overvåk innhold og områdestørrelsesendringer, oppdater automatisk rullefeltets størrelse og plassering, og skjul automatisk når det ikke er overløp.
- **Interaktiv støtte**: Støtter hjulrulling, glidebryting og posisjonering av sporklikk.
- **Statusskjul**: Vis og utvide glidebryteren når du svever eller ruller, og skjul den automatisk når den står stille.

## vertikal rulling

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

## horisontal rulling

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

## stildeler

Tilpass stilen til Shadow DOM interne elementer via `::part()`:

- `::part(scroll)`: Rull gjennom innholdsområdet.
- `::part(bar)`: Spor med rullefelt.
- `::part(si)`: glidebryter for rullefelt.
- `::part(i)`: Internt innpakningslag.
- `::part(drag)`: Rullefeltsporet i dratilstand.

## stilklasse

- `.drag`: Brukt på `body`-elementet, noe som indikerer at det er i en dra-rullingstilstand.

## Egendefinerte egenskaper

- `--si-bg`: Bakgrunnsfarge for glidebryteren.
- `--si-anim`: Skyveanimasjon.
- `--cursorScrollhSvg`: Musepekeren når det horisontale rullefeltet holdes.
- `--cursorScrollvSvg`: Musepekeren når det vertikale rullefeltet holdes.