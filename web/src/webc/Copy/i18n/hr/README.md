# Kopiraj tekst u međuspremnik

Pritisnite gumb za kopiranje teksta navedenog elementa.

- Podržava korištenje rel za određivanje selektora elementa koji će se kopirati
- Ako nije navedeno, prethodni srodni element se kopira prema zadanim postavkama.
- Ugniježđeni okvir za upit Pophover
- Međunarodna podrška

## Koristite demo

```html
<!-- Kopirajte određene elemente -->
<span id="target">Ovdje je tekst koji treba kopirati</span>
<c-copy rel="#target"></c-copy>

<!-- Prethodni elementi se kopiraju prema zadanim postavkama -->
<span>tekst prethodnog elementa</span>
<c-copy></c-copy>
```

## parametar

| imovine | ilustrirati |
| --- | --- |
| rel | Kopirajte selektor ciljnog elementa. Neobavezno |

## statusni stil

- `c-pophover.Done`: stil okvira upita nakon uspješnog kopiranja