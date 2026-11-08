# Asinhroni ielādējiet un parādiet karšu sarakstu

- **Asinhronā ielāde**: iegūstiet datus, izmantojot asinhrono funkciju
- **Statusa displejs**: atbalsta trīs stāvokļus: ielādi, tukšus datus un renderēšanas karti.
- **Elastīgs izkārtojums**: kartītes ir sakārtotas rindās

## Izmantojiet demonstrāciju

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["Nosaukums " + i, "Apraksts " + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## Interfeisa parametri

### CardLi(loader)

- **loader**: `() => Promise<Array<[string, string, string]>>`, asinhronas datu ielādes funkcija. Katras apakšatslēgas formāts ir `[nosaukums, ievads, saite]`.

## stila klase

- `.CardLi`: ārējais elements
- `.Card`: kartes elements
- `.Ing`: tiek ielādēts animēts elements