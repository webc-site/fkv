# Indikator statusa punjenja ili obrade

- Objedinite kursor u stanje čekanja
- Prikazuje animaciju mreškanja koja ukazuje na status obrade
- Primijenite masku zamućenja na obrazac kako biste spriječili dupliciranje podnošenja

## Koristite demo

```html
<!-- nezavisni indikator opterećenja -->
<div class="Ing"></div>

<!-- Obrazac za učitavanje sa maskom -->
<form class="Ing">
  <input type="text" placeholder="Kutija za unos">
  <button type="submit">submit</button>
</form>
```

## stilska klasa

### `.Ing`
- Status pokazivača je `wait`.
- Pseudo element `::before` prikazuje talasastu animaciju.
- Kada se koristi na elementu `form`, pseudoelement `::after` prikazuje masku zamućenja od 1px, onemogućujući interakciju i odabir teksta.