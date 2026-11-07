# virtuālā ritjosla

- **Divvirzienu ritināšana**: nodrošina vertikālu (`c-vs`) un horizontālu (`c-hs`) ritināšanu.
- **Izmēra atbilde**: pārraugiet satura un apgabala lieluma izmaiņas, automātiski atjauniniet ritjoslas izmēru un pozīciju un automātiski paslēpiet, ja nav pārpildes.
- **Interaktīvs atbalsts**: atbalsta riteņu ritināšanu, slīdņa vilkšanu un sliežu ceļa klikšķa pozicionēšanu.
- **Statusa slēpšana**: rādiet un paplašiniet slīdni, virzot kursoru vai ritinot, un automātiski paslēpiet to, kad tas stāv.

## vertikāla ritināšana

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

## horizontālā ritināšana

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

## stila daļas

Pielāgojiet Shadow DOM iekšējo elementu stilu, izmantojot `::part()`:

- `::part(scroll)`: ritiniet satura apgabalu.
- `::part(bar)`: ritjoslas celiņš.
- `::part(si)`: ritjoslas slīdnis.
- `::part(i)`: iekšējais iesaiņojuma slānis.
- `::part(drag)`: ritjoslas celiņš vilkšanas stāvoklī.

## stila klase

- `.drag`: tiek lietots elementam `body`, norādot, ka tas atrodas vilkšanas ritināšanas stāvoklī.

## Pielāgoti rekvizīti

- `--si-bg`: slīdņa fona krāsa.
- `--si-anim`: slīdņa animācija.
- `--cursorScrollhSvg`: peles kursors, kad tiek novietota horizontālā ritjosla.
- `--cursorScrollvSvg`: peles kursors, kad tiek novietota vertikālā ritjosla.