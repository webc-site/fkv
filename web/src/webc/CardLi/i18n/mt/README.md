# Tagħbija u turi l-lista tal-karti b'mod asinkroniku

- **Tagħbija asinkronika**: Ikseb data permezz ta' funzjoni asinkronika
- **Wiri ta 'l-Istatus**: Jappoġġja tliet stati: tagħbija, dejta vojta, u karta ta' rendering.
- **Layout Flessibbli**: Il-karti huma rranġati f'ringieli

## Uża d-demo

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["Titolu " + i, "Deskrizzjoni" + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## Parametri tal-interface

### CardLi(loader)

- **loader**: `() => Promise<Array<[string, string, string]>>`, funzjoni asinkronika tat-tagħbija tad-dejta. Il-format ta' kull subkey huwa `[isem, introduzzjoni, link]`.

## klassi tal-istil

- `.CardLi`: element ta' barra
- `.Card`: element tal-karta
- `.Ing`: Tagħbija ta' element animat