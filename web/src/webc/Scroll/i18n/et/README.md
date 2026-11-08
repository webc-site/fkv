# virtuaalne kerimisriba

- **Kahesuunaline kerimine**: pakub vertikaalset (`c-vs`) ja horisontaalset (`c-hs`) kerimist.
- **Suuruse vastus**: jälgige sisu ja ala suuruse muutusi, värskendage automaatselt kerimisriba suurust ja asukohta ning peitke automaatselt, kui ületäitumist pole.
- **Interaktiivne tugi**: toetab ratta kerimist, liuguri lohistamist ja raja klõpsamise positsioneerimist.
- **Oleku peitmine**: liuguri kuvamine ja laiendamine hõljutamisel või kerimisel ning seismisel automaatselt peitmine.

## vertikaalne kerimine

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

## horisontaalne kerimine

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

## stiiliosad

Kohandage Shadow DOM-i siseelementide stiili `::part()` kaudu:

- `::part(scroll)`: kerige sisuala.
- `::part(bar)`: kerimisriba rada.
- `::part(si)`: kerimisriba liugur.
- `::part(i)`: Sise pakendikiht.
- `::part(drag)`: kerimisriba rada lohistamise olekus.

## stiiliklass

- `.drag`: rakendatakse elemendile `body`, mis näitab, et see on lohistatavas kerimises.

## Kohandatud omadused

- `--si-bg`: liuguri taustavärv.
- `--si-anim`: Liuguri animatsioon.
- `--cursorScrollhSvg`: hiirekursor horisontaalse kerimisriba hõljutamisel.
- `--cursorScrollvSvg`: hiirekursor vertikaalse kerimisriba hõljutamisel.