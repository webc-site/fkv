# Učitavanje i prikaz liste kartica asinhrono

- **Asinhrono učitavanje**: Dobijte podatke putem asinhrone funkcije
- **Prikaz statusa**: Podržava tri stanja: učitavanje, prazni podaci i kartica za prikazivanje.
- **Fleksibilni raspored**: Kartice su raspoređene u redove

## Koristite demo

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["Naslov " + i, "Opis " + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## Parametri interfejsa

### CardLi(loader)

- **loader**: `() => Promise<Array<[string, string, string]>>`, funkcija asinkronog učitavanja podataka. Format svakog potključa je `[ime, uvod, veza]`.

## stilska klasa

- `.CardLi`: vanjski element
- `.Card`: element kartice
- `.Ing`: Učitavanje animiranog elementa