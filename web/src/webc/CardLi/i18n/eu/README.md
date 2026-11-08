# Kargatu eta bistaratu txartelen zerrenda modu asinkronoan

- **Karga asinkronoa**: Lortu datuak funtzio asinkrono baten bidez
- **Egoera bistaratzea**: hiru egoera onartzen ditu: kargatzea, datu hutsak eta errendatze txartela.
- **Diseinu malgua**: Txartelak ilaratan daude antolatuta

## Erabili demoa

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["Izenburua " + i, "Deskribapena" + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## Interfazearen parametroak

### CardLi(loader)

- **loader**: `() => Promise<Array<[string, string, string]>>`, datuak kargatzeko funtzio asinkronoa. Azpigako bakoitzaren formatua `[izena, sarrera, esteka]` da.

## estilo klasea

- `.CardLi`: kanpoko elementua
- `.Card`: txartelaren elementua
- `.Ing`: elementu animatua kargatzen