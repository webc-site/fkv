# I-load at ipakita ang listahan ng card nang hindi sabaysabay

- **Asynchronous loading**: Kumuha ng data sa pamamagitan ng isang asynchronous na function
- **Status Display**: Sinusuportahan ang tatlong estado: naglo-load, walang laman na data, at rendering card.
- **Flexible Layout**: Ang mga card ay nakaayos sa mga hilera

## Gamitin ang demo

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["Pamagat " + i, "Paglalarawan " + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## Mga parameter ng interface

### CardLi(loader)

- **loader**: `() => Promise<Array<[string, string, string]>>`, asynchronous na data loading function. Ang format ng bawat subkey ay `[pangalan, panimula, link]`.

## klase ng istilo

- `.CardLi`: panlabas na elemento
- `.Card`: elemento ng card
- `.Ing`: Naglo-load ng animated na elemento