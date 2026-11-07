# Încărcați și afișați lista de carduri în mod asincron

- **Încărcare asincronă**: obțineți date printr-o funcție asincronă
- **Afișare stare**: acceptă trei stări: încărcare, date goale și card de randare.
- **Aspect flexibil**: cardurile sunt aranjate pe rânduri

## Utilizați demonstrația

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["Titlu " + i, "Descriere" + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## Parametrii interfeței

### CardLi(loader)

- **loader**: `() => Promise<Array<[string, string, string]>>`, funcție asincronă de încărcare a datelor. Formatul fiecărei subchei este `[nume, introducere, link]`.

## clasa de stil

- `.CardLi`: element exterior
- `.Card`: element card
- `.Ing`: se încarcă elementul animat