# korritze-barra birtuala

- **Bi norabideko korritze**: korritze bertikala (`c-vs`) eta horizontala (`c-hs`) eskaintzen du.
- **Tamainaren erantzuna**: kontrolatu edukia eta eremuaren tamaina aldaketak, automatikoki eguneratu korritze-barraren tamaina eta posizioa eta automatikoki ezkutatu gainezka egiten ez denean.
- **Euskaritza interaktiboa**: gurpilen desplazamendua, graduatzailea arrastatu eta pistaren klik kokatzea onartzen du.
- **Egoera ezkutatu**: Erakutsi eta zabaldu graduatzailea pasatzean edo mugitzean, eta automatikoki ezkutatu geldirik dagoenean.

## korritze bertikala

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

## korritu horizontala

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

## estilo zatiak

Pertsonalizatu Shadow DOM barneko elementuen estiloa `::part()` bidez:

- `::part(scroll)`: joan edukiaren eremuan.
- `::part(bar)`: Korritzeko barraren pista.
- `::part(si)`: Korritzeko barraren graduatzailea.
- `::part(i)`: Barne biltzeko geruza.
- `::part(drag)`: korritze-barrako pista arrastatze-egoeran.

## estilo klasea

- `.drag`: `body` elementuari aplikatuta, arrastaka korritzeko egoeran dagoela adieraziz.

## Propietate pertsonalizatuak

- `--si-bg`: graduatzailearen atzeko planoaren kolorea.
- `--si-anim`: irristagailuaren animazioa.
- `--cursorScrollhSvg`: korritze-barra horizontala pasatzean saguaren kurtsorea.
- `--cursorScrollvSvg`: korritze-barra bertikala pasatzean saguaren kurtsorea.