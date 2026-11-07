# Indikator vum Luede oder Veraarbechtungsstatus

- Vereenegt de Cursor an de Waardestaat
- Weist Ripple Animatioun fir de Veraarbechtungsstatus unzeginn
- Fëllt eng Blur Mask op eng Form un fir duplizéiert Soumissioun ze vermeiden

## Benotzt d'Demo

```html
<!-- onofhängeg Luede Luucht -->
<div class="Ing"></div>

<!-- Luede Form mat Mask -->
<form class="Ing">
  <input type="text" placeholder="Input Këscht">
  <button type="submit">ofginn</button>
</form>
```

## Stil Klass

### `.Ing`
- Pointer Status ass `wait`.
- Pseudo-Element `::before` mécht eng Ripple-Animatioun.
- Wann se am `form` Element benotzt gëtt, mécht de Pseudo-Element `::after` eng 1px Blur Mask, déi Interaktioun an Textauswiel auszeschalten.