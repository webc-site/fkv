# Last inn og vis kortlisten asynkront

- **Asynkron lasting**: Få data gjennom en asynkron funksjon
- **Statusvisning**: Støtter tre tilstander: lasting, tom data og gjengivelseskort.
- **Fleksibel layout**: Kortene er ordnet i rader

## Bruk demoen

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["Tittel " + i, "Beskrivelse " + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## Grensesnittparametere

### CardLi(loader)

- **loader**: `() => Promise<Array<[string, string, string]>>`, funksjon for asynkron datalasting. Formatet til hver undernøkkel er `[navn, introduksjon, lenke]`.

## stilklasse

- `.CardLi`: ytre element
- `.Card`: kortelement
- `.Ing`: Laster animert element