# Kopier tekst til utklippstavlen

Klikk på knappen for å kopiere teksten til det angitte elementet.

- Støtter bruk av rel for å spesifisere elementvelgeren som skal kopieres
- Når det ikke er spesifisert, kopieres det forrige søskenelementet som standard.
- Nestet Pophover ledetekstboks
- Internasjonal støtte

## Bruk demoen

```html
<!-- Kopier spesifikke elementer -->
<span id="target">Her er teksten som skal kopieres</span>
<c-copy rel="#target"></c-copy>

<!-- Foregående elementer kopieres som standard -->
<span>teksten til forrige element</span>
<c-copy></c-copy>
```

## parameter

| eiendom | illustrere |
| --- | --- |
| rel | Kopier velgeren til målelementet. Valgfri |

## statusstil

- `c-pophover.Done`: ledetekststil etter vellykket kopiering