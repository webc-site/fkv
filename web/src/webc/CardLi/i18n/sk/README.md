# Načítať a zobraziť zoznam kariet asynchrónne

- **Asynchrónne načítanie**: Získajte údaje prostredníctvom asynchrónnej funkcie
- **Zobrazenie stavu**: Podporuje tri stavy: načítanie, vyprázdnenie údajov a vykresľovacia karta.
- **Flexibilné rozloženie**: Karty sú usporiadané v radoch

## Použite demo

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["Názov " + i, "Popis" + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## Parametre rozhrania

### CardLi(loader)

- **loader**: `() => Promise<Array<[string, string, string]>>`, funkcia asynchrónneho načítania údajov. Formát každého podkľúča je `[názov, úvod, odkaz]`.

## štýlová trieda

- `.CardLi`: vonkajší prvok
- `.Card`: prvok karty
- `.Ing`: Načítava sa animovaný prvok