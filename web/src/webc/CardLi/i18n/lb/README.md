# Lueden a weisen Kaartelëscht asynchron

- ** Asynchron Luede **: Kritt Daten duerch eng asynchron Funktioun
- ** Status Display **: Ënnerstëtzt dräi Staaten: Luede, eidel Daten, a Rendering Kaart.
- ** Flexibel Layout **: Kaarte sinn an Zeile arrangéiert

## Benotzt d'Demo

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["Titel " + i, "Beschreiwung " + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## Interface Parameteren

### CardLi(loader)

- **Loader**: `() => Promise<Array<[string, string, string]>>`, asynchrone Datenlaaschtfunktioun. D'Format vun all Ënnerschlëssel ass `[Numm, Aféierung, Link]`.

## Stil Klass

- `.CardLi`: baussenzegen Element
- `.Card`: Kaartelement
- `.Ing`: Animéiert Element lueden