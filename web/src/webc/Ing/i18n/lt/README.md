# Pakrovimo arba apdorojimo būsenos indikatorius

- Sujunkite žymeklį į laukimo būseną
- Rodoma bangavimo animacija, nurodanti apdorojimo būseną
- Formai uždėkite suliejimo kaukę, kad išvengtumėte pasikartojančių pateikimų

## Naudokite demonstracinę versiją

```html
<!-- nepriklausomas pakrovimo indikatorius -->
<div class="Ing"></div>

<!-- Formos pakrovimas su kauke -->
<form class="Ing">
  <input type="text" placeholder="Įvesties dėžutė">
  <button type="submit">pateikti</button>
</form>
```

## stiliaus klasė

### `.Ing`
- Žymeklio būsena yra `wait`.
- Pseudoelementas `::before` atvaizduoja bangavimo animaciją.
- Kai naudojamas `form` elemente, pseudoelementas `::after` pateikia 1 piks. suliejimo kaukę, išjungiančią sąveiką ir teksto pasirinkimą.