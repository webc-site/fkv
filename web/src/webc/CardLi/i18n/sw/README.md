# Pakia na uonyeshe orodha ya kadi bila mpangilio

- **Upakiaji usiolingana**: Pata data kupitia kitendakazi kisicholingana
- **Onyesho la Hali**: Inaauni hali tatu: upakiaji, data tupu, na kadi ya uwasilishaji.
- **Mpangilio Unaonyumbulika**: Kadi zimepangwa kwa safu

## Tumia onyesho

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["Kichwa " + i, "Maelezo " + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## Vigezo vya interface

### CardLi(loader)

- ** kipakiaji**: `() => Promise<Array<[string, string, string]>>`, kitendakazi cha upakiaji data kisicholingana. Umbizo la kila ufunguo mdogo ni `[jina, utangulizi, kiungo]`.

## darasa la mtindo

- `.CardLi`: kipengele cha nje
- `.Card`: kipengele cha kadi
- `.Ing`: Inapakia kipengele cha uhuishaji