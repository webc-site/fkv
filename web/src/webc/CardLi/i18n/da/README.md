# Indlæs og vis kortlisten asynkront

- **Asynkron indlæsning**: Få data gennem en asynkron funktion
- **Statusvisning**: Understøtter tre tilstande: indlæsning, tom data og gengivelseskort.
- **Fleksibelt layout**: Kortene er arrangeret i rækker

## Brug demoen

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["Titel " + i, "Beskrivelse " + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## Interface parametre

### CardLi(loader)

- **loader**: `() => Promise<Array<[string, string, string]>>`, asynkron dataindlæsningsfunktion. Formatet for hver undernøgle er `[navn, introduktion, link]`.

## stil klasse

- `.CardLi`: ydre element
- `.Card`: kortelement
- `.Ing`: Indlæser animeret element