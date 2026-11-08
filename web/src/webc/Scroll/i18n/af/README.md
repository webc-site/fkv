# virtuele skuifbalk

- **Tweerigting-blaai**: Verskaf vertikale (`c-vs`) en horisontale (`c-hs`) blaai.
- **Groottereaksie**: Monitor inhoud en areagrootteveranderinge, dateer die rolbalkgrootte en posisie outomaties op, en versteek outomaties wanneer daar geen oorloop is nie.
- **Interaktiewe ondersteuning**: Ondersteun wielblaai, sleepskuif en spoorkliekposisionering.
- **Status versteek**: Wys en verbreed die skuifbalk wanneer jy beweeg of blaai, en versteek dit outomaties wanneer dit stilstaan.

## vertikale blaai

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

## horisontale blaai

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

## styl dele

Pas die styl van Shadow DOM interne elemente aan via `::part()`:

- `::part(scroll)`: Blaai deur die inhoudarea.
- `::part(bar)`: Skuifbalksnit.
- `::part(si)`: Skuifbalkskuif.
- `::part(i)`: Interne toevoulaag.
- `::part(drag)`: Die rolbalkbaan in die sleeptoestand.

## styl klas

- `.drag`: Toegepas op die `body` element, wat aandui dat dit in 'n sleep-blaai toestand is.

## Pasgemaakte eiendomme

- `--si-bg`: Slider agtergrondkleur.
- `--si-anim`: Slider-animasie.
- `--cursorScrollhSvg`: Die muiswyser wanneer die horisontale skuifbalk gesweef word.
- `--cursorScrollvSvg`: Die muiswyser wanneer die vertikale skuifbalk beweeg.