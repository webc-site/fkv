# Toa na uonyeshe onyesho za sehemu, uwekaji kumbukumbu, na maagizo ya usakinishaji

- Chambua na uchanganue kichwa, utangulizi na maudhui ya waraka wa README
- Sandbox ili kuendesha maonyesho shirikishi ya vipengele na mitindo ya kutenga
- Badilisha vizuizi vya msimbo katika hati kuwa mawasilisho shirikishi
- Mpangilio unaojirekebisha, onyesho la safu wima ya skrini pana, onyesho nyembamba la kubadilisha kichupo

## Tumia onyesho

```html
<c-webc-demo></c-webc-demo>
```

```js
import WebCDemo from "./_.js";

const el = document.querySelector("c-webc-demo");

// Anzisha na upate kiolesura cha uendeshaji
const [setMod, setMd, unMount] = WebCDemo(el);
```

## Vigezo vya interface

### `WebCDemo(el)`

Anzisha kipengele.

- **kigezo**
  - `el`:`HTMLElement`, chombo cha kijenzi.
- **Thamani ya kurejesha**:`[setMod, setMd, unMount]`
  - `setMod(mod)`: Sanidi moduli ya onyesho.
  - `setMd(name, md, lang)`: Weka jina la kijenzi, maudhui ya hati ya README na lugha ya sasa.
  - `unMount()`: Pakua kijenzi na usafishe msikilizaji wa tukio.

### Sehemu ya onyesho `mod` sifa

- `CSS`:`string`, njia ya laha ya mtindo wa kijenzi.
- `HTM`:`string`, kiolezo cha kijenzi cha HTML.
- `default`:`function(root)`, kitendakazi cha uanzishaji wa kijenzi, hupokea kontena la mzizi wa kivuli.

## darasa la mtindo

### `.M`

eneo la maudhui.

### `.L`

Paneli ya kushoto ina maagizo na nyaraka.

### `.R`

Paneli ya kulia, iliyo na onyesho.

### `c-nav`

Upau wa kusogeza wa skrini nyembamba.