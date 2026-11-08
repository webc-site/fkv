# I-render ug ipakita ang mga demo sa sangkap, dokumentasyon, ug mga panudlo sa pag-install

- Kuhaa ug i-parse ang titulo, pasiuna ug sulod sa README nga dokumento
- Sandbox aron magpadagan sa mga interactive nga demo sa mga sangkap ug ibulag ang mga istilo
- I-convert ang mga bloke sa code sa mga dokumento ngadto sa interactive nga mga presentasyon
- Adaptive nga layout, lapad nga screen nga kolum nga pagpakita, pig-ot nga screen tab switching display

## Gamita ang demo

```html
<c-webc-demo></c-webc-demo>
```

```js
import WebCDemo from "./_.js";

const el = document.querySelector("c-webc-demo");

// Pagsugod ug pagkuha sa interface sa operasyon
const [setMod, setMd, unMount] = WebCDemo(el);
```

## Mga parametro sa interface

### `WebCDemo(el)`

Pagsugod sa component.

- **parameter**
  - `el`:`HTMLElement`, sangkap nga sudlanan.
- **Balik nga bili**:`[setMod, setMd, unMount]`
  - `setMod(mod)`: I-set up ang demo module.
  - `setMd(name, md, lang)`: Itakda ang component name, README document content ug current language.
  - `unMount()`: Idiskarga ang component ug limpyohi ang tigpaminaw sa panghitabo.

### Demo module `mod` attribute

- `CSS`:`string`, ang style sheet nga agianan sa component.
- `HTM`:`string`, ang HTML template sa component.
- `default`:`function(root)`, component initialization function, makadawat og shadow root container.

## estilo nga klase

### `.M`

sulod nga dapit.

### `.L`

Ang wala nga panel adunay mga panudlo ug dokumentasyon.

### `.R`

Tuo nga panel, nga adunay demo.

### `c-nav`

Pikit nga screen navigation bar.