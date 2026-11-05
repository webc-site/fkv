# Llwytho ac arddangos rhestr cardiau yn anghydamserol

- **Llwytho asyncronig**: Cael data trwy swyddogaeth asyncronig
- **Arddangosfa Statws **: Yn cefnogi tri chyflwr: llwytho, gwagio data, a cherdyn rendro.
- **Cynllun Hyblyg**: Trefnir cardiau mewn rhesi

## Defnyddiwch y demo

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["Teitl" + i, "Disgrifiad" + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## Paramedrau rhyngwyneb

### CardLi(loader)

- **loader**: `() => Promise<Array<[string, string, string]>>`, swyddogaeth llwytho data asyncronaidd. Fformat pob subkey yw `[enw, cyflwyniad, dolen]`.

## dosbarth arddull

- `.CardLi`: elfen allanol
- `.Card`: elfen cerdyn
- `.Ing`: Yn llwytho elfen animeiddiedig