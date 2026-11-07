# Asinhrono nalaganje in prikaz seznama kartic

- **Asinhrono nalaganje**: pridobite podatke prek asinhrone funkcije
- **Prikaz stanja**: podpira tri stanja: nalaganje, prazni podatki in upodabljanje kartice.
- **Prilagodljiva postavitev**: Karte so razvrščene v vrstice

## Uporabite demo

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

## Parametri vmesnika

### CardLi(loader)

- **nalagalnik**: `() => Promise<Array<[string, string, string]>>`, funkcija asinhronega nalaganja podatkov. Format vsakega podključa je `[ime, uvod, povezava]`.

## stilski razred

- `.CardLi`: zunanji element
- `.Card`: element kartice
- `.Ing`: Nalaganje animiranega elementa