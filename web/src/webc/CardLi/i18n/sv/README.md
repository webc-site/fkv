# Ladda och visa kortlistan asynkront

- **Asynkron laddning**: Få data via en asynkron funktion
- **Statusvisning**: Stöder tre tillstånd: laddning, tom data och renderingskort.
- **Flexibel layout**: Korten är ordnade i rader

## Använd demon

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["Titel " + i, "Beskrivning " + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## Gränssnittsparametrar

### CardLi(loader)

- **loader**: `() => Promise<Array<[string, string, string]>>`, asynkron dataladdningsfunktion. Formatet för varje undernyckel är `[namn, introduktion, länk]`.

## stilklass

- `.CardLi`: yttre element
- `.Card`: kortelement
- `.Ing`: Laddar animerat element