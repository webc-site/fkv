# Indikator for indlæsnings- eller behandlingsstatus

- Foren markøren til ventetilstand
- Viser krusningsanimation for at angive behandlingsstatus
- Anvend en sløringsmaske på en formular for at forhindre duplikerede indsendelser

## Brug demoen

```html
<!-- uafhængig belastningsindikator -->
<div class="Ing"></div>

<!-- Indlæsningsform med maske -->
<form class="Ing">
  <input type="text" placeholder="Indtastningsboks">
  <button type="submit">indsende</button>
</form>
```

## stil klasse

### `.Ing`
- Pegerstatus er `wait`.
- Pseudoelement `::before` gengiver en krusningsanimation.
- Når det bruges på `form` elementet, gengiver pseudoelementet `::after` en 1px sløringsmaske, hvilket deaktiverer interaktion og tekstvalg.