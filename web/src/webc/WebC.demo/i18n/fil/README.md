# I-render at ipakita ang mga demo ng bahagi, dokumentasyon, at mga tagubilin sa pag-install

- I-extract at i-parse ang pamagat, panimula at nilalaman ng README na dokumento
- Sandbox upang magpatakbo ng mga interactive na demo ng mga bahagi at ihiwalay ang mga istilo
- I-convert ang mga bloke ng code sa mga dokumento sa mga interactive na presentasyon
- Adaptive na layout, wide screen column display, makitid na screen tab switching display

## Gamitin ang demo

```html
<c-webc-demo></c-webc-demo>
```

```js
import WebCDemo from "./_.js";

const el = document.querySelector("c-webc-demo");

//Initialize at makuha ang operation interface
const [setMod, setMd, unMount] = WebCDemo(el);
```

## Mga parameter ng interface

### `WebCDemo(el)`

Unahin ang bahagi.

- **parameter**
  - `el`:`HTMLElement`, lalagyan ng bahagi.
- **Return value**:`[setMod, setMd, unMount]`
  - `setMod(mod)`: I-set up ang demo module.
  - `setMd(name, md, lang)`: Itakda ang pangalan ng bahagi, nilalaman ng dokumento ng README at kasalukuyang wika.
  - `unMount()`: I-unload ang bahagi at linisin ang tagapakinig ng kaganapan.

### Demo module `mod` attribute

- `CSS`:`string`, ang style sheet path ng component.
- `HTM`:`string`, ang HTML template ng component.
- `default`:`function(root)`, component initialization function, tumatanggap ng shadow root container.

## klase ng istilo

### `.M`

lugar ng nilalaman.

### `.L`

Ang kaliwang panel ay naglalaman ng mga tagubilin at dokumentasyon.

### `.R`

Kanang panel, na naglalaman ng demo.

### `c-nav`

Makitid na screen navigation bar.