# Načíst a zobrazit seznam karet asynchronně

- **Asynchronní načítání**: Získejte data prostřednictvím asynchronní funkce
- **Zobrazení stavu**: Podporuje tři stavy: načítání, vyprázdnění dat a vykreslovací karta.
- **Flexibilní rozvržení**: Karty jsou uspořádány v řadách

## Použijte demo

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["Název " + i, "Popis" + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## Parametry rozhraní

### CardLi(loader)

- **loader**: `() => Promise<Array<[string, string, string]>>`, funkce asynchronního načítání dat. Formát každého podklíče je `[jméno, úvod, odkaz]`.

## stylová třída

- `.CardLi`: vnější prvek
- `.Card`: prvek karty
- `.Ing`: Načítání animovaného prvku