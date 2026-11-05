# I-load ug ipakita ang lista sa kard nga asynchronous

- **Asynchronous loading**: Pagkuhag data pinaagi sa asynchronous nga function
- **Status Display**: Nagsuporta sa tulo ka estado: loading, walay sulod nga data, ug rendering card.
- **Flexible Layout**: Ang mga kard gihan-ay sa mga laray

## Gamita ang demo

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["Titulo " + i, "Deskripsyon " + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## Mga parametro sa interface

### CardLi(loader)

- **loader**: `() => Promise<Array<[string, string, string]>>`, asynchronous nga data loading function. Ang pormat sa matag subkey kay `[ngalan, introduksiyon, link]`.

## estilo nga klase

- `.CardLi`: panggawas nga elemento
- `.Card`: elemento sa kard
- `.Ing`: Nagkarga sa animated nga elemento