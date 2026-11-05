# Lataa ja näytä korttiluettelo asynkronisesti

- **Asynkroninen lataus**: Hae tiedot asynkronisen toiminnon kautta
- **Tilan näyttö**: Tukee kolmea tilaa: lataus, tyhjä data ja renderöintikortti.
- **Joustava asettelu**: Kortit on järjestetty riveihin

## Käytä demoa

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["Otsikko " + i, "Kuvaus" + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## Käyttöliittymän parametrit

### CardLi(loader)

- **loader**: `() => Promise<Array<[string, string, string]>>`, asynkroninen tiedonlataustoiminto. Kunkin aliavaimen muoto on `[nimi, johdanto, linkki]`.

## tyyliluokka

- `.CardLi`: ulompi elementti
- `.Card`: korttielementti
- `.Ing`: Ladataan animoitua elementtiä