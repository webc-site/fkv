# Kopier tekst til udklipsholder

Klik på knappen for at kopiere teksten til det angivne element.

- Understøtter brug af rel til at specificere elementvælgeren, der skal kopieres
- Når det ikke er angivet, kopieres det forrige søskendeelement som standard.
- Indlejret Pophover-promptboks
- International støtte

## Brug demoen

```html
<!-- Kopier specifikke elementer -->
<span id="target">Her er teksten, der skal kopieres</span>
<c-copy rel="#target"></c-copy>

<!-- Foregående elementer kopieres som standard -->
<span>teksten til det forrige element</span>
<c-copy></c-copy>
```

## parameter

| ejendom | illustrere |
| --- | --- |
| rel | Kopiér vælgeren for målelementet. Valgfri |

## statusstil

- `c-pophover.Done`: promptboksstil efter vellykket kopiering