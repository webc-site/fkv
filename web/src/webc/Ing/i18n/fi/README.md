# Latauksen tai käsittelyn tilan ilmaisin

- Yhdistä kohdistin odotustilaan
- Näyttää aaltoilun animaation ilmaisemaan käsittelyn tilan
- Lisää lomakkeeseen sumennusmaski estääksesi päällekkäiset lähetykset

## Käytä demoa

```html
<!-- itsenäinen latausilmaisin -->
<div class="Ing"></div>

<!-- Lomakkeen lataus maskin kanssa -->
<form class="Ing">
  <input type="text" placeholder="Syöttölaatikko">
  <button type="submit">lähetä</button>
</form>
```

## tyyliluokka

### `.Ing`
- Osoittimen tila on `wait`.
- Pseudoelementti `::before` tekee aaltoiluanimaatiosta.
- Käytettäessä `form`-elementissä pseudoelementti `::after` tekee 1px-sumennusmaskin, joka estää vuorovaikutuksen ja tekstin valinnan.