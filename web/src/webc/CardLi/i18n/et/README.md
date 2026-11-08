# Laadige ja kuvage kaartide loend asünkroonselt

- **Asünkroonne laadimine**: hankige andmeid asünkroonse funktsiooni kaudu
- **Olukuva**: toetab kolme olekut: laadimine, tühjad andmed ja renderduskaart.
- **Paindlik paigutus**: kaardid on paigutatud ridadesse

## Kasutage demo

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["Pealkiri " + i, "Kirjeldus " + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## Liidese parameetrid

### CardLi(loader)

- **loader**: `() => Promise<Array<[string, string, string]>>`, asünkroonne andmete laadimise funktsioon. Iga alamvõtme vorming on `[nimi, tutvustus, link]`.

## stiiliklass

- `.CardLi`: välimine element
- `.Card`: kaardi element
- `.Ing`: animeeritud elemendi laadimine