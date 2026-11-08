# Kopirajte tekst u međuspremnik

Kliknite na dugme da biste kopirali tekst navedenog elementa.

- Podržava korištenje rel za određivanje selektora elemenata koji će se kopirati
- Kada nije naveden, prethodni srodni element se kopira po defaultu.
- Ugniježđeni Pophover okvir za prompt
- Međunarodna podrška

## Koristite demo

```html
<!-- Kopirajte određene elemente -->
<span id="target">Evo teksta za kopiranje</span>
<c-copy rel="#target"></c-copy>

<!-- Prethodni elementi se kopiraju po defaultu -->
<span>tekst prethodnog elementa</span>
<c-copy></c-copy>
```

## parametar

| imovine | ilustrirati |
| --- | --- |
| rel | Kopirajte selektor ciljnog elementa. Opciono |

## statusni stil

- `c-pophover.Done`: stil okvira upita nakon uspješnog kopiranja