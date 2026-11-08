# shiriti i lëvizjes virtuale

- **Lëvizja me dy drejtime**: Ofron lëvizje vertikale (`c-vs`) dhe horizontale (`c-hs`).
- **Përgjigja e madhësisë**: Monitoroni ndryshimet e përmbajtjes dhe madhësisë së zonës, përditësoni automatikisht madhësinë dhe pozicionin e shiritit të lëvizjes dhe fshiheni automatikisht kur nuk ka tejmbushje.
- **Mbështetje ndërvepruese**: Mbështet lëvizjen e rrotave, zvarritjen e rrëshqitësit dhe pozicionimin e klikimeve të gjurmimit.
- **Status Hide**: Shfaq dhe zgjero rrëshqitësin kur rri pezull ose lëviz dhe fshihet automatikisht kur është i palëvizshëm.

## rrotull vertikale

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

## rrotull horizontale

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

## pjesë të stilit

Personalizo stilin e elementeve të brendshëm Shadow DOM nëpërmjet `::part()`:

- `::part(scroll)`: Lëvizni zonën e përmbajtjes.
- `::part(bar)`: gjurmët e shiritit të lëvizjes.
- `::part(si)`: rrëshqitësi i shiritit të lëvizjes.
- `::part(i)`: Shtresa e brendshme e mbështjelljes.
- `::part(drag)`: Gjurma e shiritit të lëvizjes në gjendje zvarritjeje.

## klasë stili

- `.drag`: Zbatohet në elementin `body`, duke treguar se është në gjendje zvarritjeje.

## Karakteristikat e personalizuara

- `--si-bg`: Ngjyra e sfondit të rrëshqitësit.
- `--si-anim`: Animacion rrëshqitës.
- `--cursorScrollhSvg`: Kursori i miut kur rri pezull shiritin e lëvizjes horizontale.
- `--cursorScrollvSvg`: Kursori i miut kur shiriti i lëvizjes vertikale qëndron pezull.