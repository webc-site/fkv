# Timailhan sa loading o pagproseso sa kahimtang

- Iusa ang cursor sa kahimtang sa paghulat
- Nagpakita sa ripple animation aron ipakita ang kahimtang sa pagproseso
- Ibutang ang usa ka blur mask sa usa ka porma aron malikayan ang mga doble nga pagsumite

## Gamita ang demo

```html
<!-- independente nga loading indicator -->
<div class="Ing"></div>

<!-- Pag-load sa porma nga adunay maskara -->
<form class="Ing">
  <input type="text" placeholder="Input nga kahon">
  <button type="submit">isumiter</button>
</form>
```

## estilo nga klase

### `.Ing`
- Ang kahimtang sa pointer mao ang `wait`.
- Ang pseudo nga elemento `::before` naghimo og ripple animation.
- Kung gigamit sa `form` nga elemento, ang pseudo-element nga `::after` muhatag ug 1px blur mask, dili pagpagana sa interaksyon ug pagpili sa teksto.