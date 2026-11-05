# Rendro ac arddangos demos cydrannau, dogfennaeth, a chyfarwyddiadau gosod

- Echdynnu a dosrannu teitl, cyflwyniad a chynnwys y ddogfen README
- Blwch tywod i redeg demos rhyngweithiol o gydrannau ac ynysu arddulliau
- Trosi blociau cod mewn dogfennau yn gyflwyniadau rhyngweithiol
- Cynllun addasol, arddangosfa golofn sgrin lydan, arddangosfa newid tab sgrin gul

## Defnyddiwch y demo

```html
<c-webc-demo></c-webc-demo>
```

```js
import WebCDemo from "./_.js";

const el = document.querySelector("c-webc-demo");

// Cychwyn a chael y rhyngwyneb gweithredu
const [setMod, setMd, unMount] = WebCDemo(el);
```

## Paramedrau rhyngwyneb

### `WebCDemo(el)`

Cychwyn y gydran.

- **paramedr**
  - `el`:`HTMLElement`, cynhwysydd cydran.
- **Gwerth dychwelyd**:`[setMod, setMd, unMount]`
  - `setMod(mod)`: Gosodwch y modiwl demo.
  - `setMd(name, md, lang)`: Gosod enw'r gydran, cynnwys y ddogfen README a'r iaith gyfredol.
  - `unMount()`: Dadlwythwch y gydran a glanhau gwrandäwr y digwyddiad.

### Modiwl demo `mod` priodoledd

- `CSS`:`string`, llwybr dalen arddull y gydran.
- `HTM`:`string`, templed HTML y gydran.
- `default`: `function(root)`, swyddogaeth ymgychwyn cydran, yn derbyn cynhwysydd gwraidd cysgodol.

## dosbarth arddull

### `.M`

maes cynnwys.

### `.L`

Mae'r panel chwith yn cynnwys cyfarwyddiadau a dogfennaeth.

### `.R`

Panel ar y dde, yn cynnwys y demo.

### `c-nav`

Bar llywio sgrin gul.