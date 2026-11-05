# Asinchroniškai įkelkite ir rodykite kortelių sąrašą

- **Asinchroninis įkėlimas**: gaukite duomenis naudodami asinchroninę funkciją
- **Būsenos ekranas**: palaiko tris būsenas: įkėlimą, tuščius duomenis ir atvaizdavimo kortelę.
- **Lankstus išdėstymas**: kortelės išdėstytos eilėmis

## Naudokite demonstracinę versiją

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["Pavadinimas " + i, "Aprašymas " + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## Sąsajos parametrai

### CardLi(loader)

- **kroviklis**: `() => Promise<Array<[string, string, string]>>`, asinchroninio duomenų įkėlimo funkcija. Kiekvieno dalinio rakto formatas yra `[vardas, įvadas, nuoroda]`.

## stiliaus klasė

- `.CardLi`: išorinis elementas
- `.Card`: kortelės elementas
- `.Ing`: įkeliamas animuotas elementas