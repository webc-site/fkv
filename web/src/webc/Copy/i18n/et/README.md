# Kopeerige tekst lõikelauale

Määratud elemendi teksti kopeerimiseks klõpsake nuppu.

- Toetab rel kasutamist kopeeritava elemendi valija määramiseks
- Kui seda pole määratud, kopeeritakse vaikimisi eelmine õdeelement.
- Pesastatud Pophoveri viibakast
- Rahvusvaheline tugi

## Kasutage demo

```html
<!-- Kopeerige konkreetsed elemendid -->
<span id="target">Siin on kopeeritav tekst</span>
<c-copy rel="#target"></c-copy>

<!-- Eelnevad elemendid kopeeritakse vaikimisi -->
<span>eelmise elemendi tekst</span>
<c-copy></c-copy>
```

## parameeter

| vara | illustreerima |
| --- | --- |
| rel | Kopeerige sihtelemendi valija. Valikuline |

## oleku stiil

- `c-pophover.Done`: viipakasti stiil pärast edukat kopeerimist