# Indikator for lasting eller behandlingsstatus

- Foren markøren til ventetilstand
- Viser krusningsanimasjon for å indikere behandlingsstatus
- Bruk en uskarphetsmaske på et skjema for å forhindre dupliserte innsendinger

## Bruk demoen

```html
<!-- uavhengig lasteindikator -->
<div class="Ing"></div>

<!-- Lasteskjema med maske -->
<form class="Ing">
  <input type="text" placeholder="Inndataboks">
  <button type="submit">sende inn</button>
</form>
```

## stilklasse

### `.Ing`
- Pekerstatus er `wait`.
- Pseudoelement `::before` gjengir en krusningsanimasjon.
- Når det brukes på `form`-elementet, gjengir pseudoelementet `::after` en 1px uskarphetsmaske, og deaktiverer interaksjon og tekstvalg.