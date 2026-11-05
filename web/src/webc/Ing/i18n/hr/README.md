# Indikator statusa učitavanja ili obrade

- Ujedinite kursor u stanje čekanja
- Prikazuje valovitu animaciju za označavanje statusa obrade
- Primijenite masku zamućenja na obrazac kako biste spriječili duple prijave

## Koristite demo

```html
<!-- neovisni indikator opterećenja -->
<div class="Ing"></div>

<!-- Obrazac za učitavanje s maskom -->
<form class="Ing">
  <input type="text" placeholder="Okvir za unos">
  <button type="submit">podnijeti</button>
</form>
```

## stilska klasa

### `.Ing`
- Status pokazivača je `wait`.
- Pseudoelement `::before` renderira animaciju mreškanja.
- Kada se koristi na elementu `form`, pseudoelement `::after` renderira masku zamućenja od 1 px, onemogućujući interakciju i odabir teksta.