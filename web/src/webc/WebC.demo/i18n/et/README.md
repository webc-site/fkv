# Renderdage ja kuvage komponentide demosid, dokumentatsiooni ja paigaldusjuhiseid

- Tõmmake välja ja analüüsige README dokumendi pealkiri, sissejuhatus ja sisu
- Liivakast komponentide interaktiivsete demode käitamiseks ja stiilide isoleerimiseks
- Teisendage dokumentides olevad koodiplokid interaktiivseteks esitlusteks
- Kohanduv paigutus, laiekraani veeru kuva, kitsa ekraani vahekaardi vahetamise ekraan

## Kasutage demo

```html
<c-webc-demo></c-webc-demo>
```

```js
import WebCDemo from "./_.js";

const el = document.querySelector("c-webc-demo");

//Initsialiseerige ja hankige operatsiooniliides
const [setMod, setMd, unMount] = WebCDemo(el);
```

## Liidese parameetrid

### `WebCDemo(el)`

Initsialiseerige komponent.

- **parameeter**
  - `el`:`HTMLElement`, komponendi konteiner.
- **Tagastusväärtus**:`[setMod, setMd, unMount]`
  - `setMod(mod)`: seadistage demomoodul.
  - `setMd(name, md, lang)`: määrake komponendi nimi, README dokumendi sisu ja praegune keel.
  - `unMount()`: laadige komponent maha ja puhastage sündmuste kuulaja.

### Demomooduli `mod` atribuut

- `CSS`:`string`, komponendi laadilehe tee.
- `HTM`:`string`, komponendi HTML-mall.
- `default`:`function(root)`, komponendi lähtestamisfunktsioon, võtab vastu varijuurkonteineri.

## stiiliklass

### `.M`

sisuala.

### `.L`

Vasakpoolne paneel sisaldab juhiseid ja dokumentatsiooni.

### `.R`

Parem paneel, mis sisaldab demo.

### `c-nav`

Kitsa ekraaniga navigeerimisriba.