# Laai en vertoon kaartlys asinchronies

- **Asinchroniese laai**: Kry data deur 'n asinchrone funksie
- **Statusvertoning**: Ondersteun drie toestande: laai, leë data en lewering van kaart.
- **Buigsame uitleg**: Kaarte word in rye gerangskik

## Gebruik die demo

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["Titel " + i, "Beskrywing " + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## Interface parameters

### CardLi(loader)

- **laaier**: `() => Promise<Array<[string, string, string]>>`, asinchroniese datalaaifunksie. Die formaat van elke subsleutel is `[naam, inleiding, skakel]`.

## styl klas

- `.CardLi`: buitenste element
- `.Card`: kaartelement
- `.Ing`: Laai geanimeerde element