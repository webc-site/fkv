# Nukopijuokite tekstą į mainų sritį

Spustelėkite mygtuką, kad nukopijuotumėte nurodyto elemento tekstą.

- Palaikomas rel naudojimas norint nurodyti kopijuojamą elementų parinkiklį
- Kai nenurodyta, ankstesnis brolio elementas nukopijuojamas pagal numatytuosius nustatymus.
- Įdėtas Pophover raginimo laukelis
- Tarptautinė parama

## Naudokite demonstracinę versiją

```html
<!-- Nukopijuokite konkrečius elementus -->
<span id="target">Čia yra tekstas, kurį reikia nukopijuoti</span>
<c-copy rel="#target"></c-copy>

<!-- Ankstesni elementai nukopijuojami pagal numatytuosius nustatymus -->
<span>ankstesnio elemento tekstas</span>
<c-copy></c-copy>
```

## parametras

| nuosavybė | iliustruoti |
| --- | --- |
| rel | Nukopijuokite tikslinio elemento parinkiklį. Neprivaloma |

## statuso stilius

- `c-pophover.Done`: raginimo dėžutės stilius po sėkmingo kopijavimo