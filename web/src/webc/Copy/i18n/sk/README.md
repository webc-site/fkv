# Skopírujte text do schránky

Kliknutím na tlačidlo skopírujete text zadaného prvku.

- Podporuje použitie rel na určenie selektora prvkov, ktorý sa má skopírovať
- Ak nie je zadaný, predvolene sa skopíruje predchádzajúci súrodenecký prvok.
- Vnorené pole výzvy Pophover
- Medzinárodná podpora

## Použite demo

```html
<!-- Skopírujte konkrétne prvky -->
<span id="target">Tu je text, ktorý sa má skopírovať</span>
<c-copy rel="#target"></c-copy>

<!-- Predchádzajúce prvky sa štandardne skopírujú -->
<span>text predchádzajúceho prvku</span>
<c-copy></c-copy>
```

## parameter

| majetku | ilustrovať |
| --- | --- |
| rel | Skopírujte selektor cieľového prvku. Voliteľné |

## stavový štýl

- `c-pophover.Done`: štýl poľa výzvy po úspešnom skopírovaní