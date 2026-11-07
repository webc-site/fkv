# Indicator al stării de încărcare sau procesare

- Unificați cursorul în starea de așteptare
- Afișează o animație ondulată pentru a indica starea procesării
- Aplicați o mască de estompare unui formular pentru a preveni trimiterile duble

## Utilizați demonstrația

```html
<!-- indicator independent de încărcare -->
<div class="Ing"></div>

<!-- Se incarca formularul cu masca -->
<form class="Ing">
  <input type="text" placeholder="Caseta de intrare">
  <button type="submit">depune</button>
</form>
```

## clasa de stil

### `.Ing`
- Starea indicatorului este `wait`.
- Pseudoelementul `::before` redă o animație ondulată.
- Când este utilizat pe elementul `form`, pseudo-elementul `::after` redă o mască de estompare de 1 px, dezactivând interacțiunea și selecția textului.