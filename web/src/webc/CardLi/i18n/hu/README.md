# A kártyalista aszinkron betöltése és megjelenítése

- **Aszinkron betöltés**: Adatok lekérése aszinkron függvényen keresztül
- **Állapot kijelző**: Három állapotot támogat: betöltés, üres adatok és kártya megjelenítése.
- **Rugalmas elrendezés**: A kártyák sorokba vannak rendezve

## Használd a demót

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["Cím " + i, "Leírás " + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## Interfész paraméterei

### CardLi(loader)

- **betöltő**: `() => Promise<Array<[string, string, string]>>`, aszinkron adatbetöltő funkció. Az egyes alkulcsok formátuma: `[név, bevezetés, hivatkozás]`.

## stílus osztály

- `.CardLi`: külső elem
- `.Card`: kártyaelem
- `.Ing`: Animált elem betöltése