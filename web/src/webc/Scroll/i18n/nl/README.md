# virtuele schuifbalk

- **Tweerichtingsscrollen**: Biedt verticaal (`c-vs`) en horizontaal (`c-hs`) scrollen.
- **Groottereactie**: houd wijzigingen in de inhoud en gebiedsgrootte bij, werk automatisch de grootte en positie van de schuifbalk bij en verberg deze automatisch als er geen overloop is.
- **Interactieve ondersteuning**: Ondersteunt scrollen met wielen, slepen van schuifregelaars en positioneren van trackklikken.
- **Status verbergen**: Toon en verbreed de schuifregelaar tijdens het zweven of scrollen, en verberg deze automatisch wanneer deze stilstaat.

## verticale scroll

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

## horizontale scroll

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

## stijl onderdelen

Pas de stijl van interne Shadow DOM-elementen aan via `::part()`:

- `::part(scroll)`: blader door het inhoudsgebied.
- `::part(bar)`: schuifbalktrack.
- `::part(si)`: Schuifbalkschuifregelaar.
- `::part(i)`: interne wikkellaag.
- `::part(drag)`: Het schuifbalkspoor in sleepstatus.

## stijl klasse

- `.drag`: toegepast op het `body` element, wat aangeeft dat het zich in een drag-scroll-status bevindt.

## Aangepaste eigenschappen

- `--si-bg`: achtergrondkleur van de schuifregelaar.
- `--si-anim`: schuifbalkanimatie.
- `--cursorScrollhSvg`: De muiscursor wanneer de horizontale schuifbalk zweeft.
- `--cursorScrollvSvg`: De muiscursor wanneer de verticale schuifbalk zweeft.