# Irrendi u juri demos tal-komponenti, dokumentazzjoni, u struzzjonijiet ta 'installazzjoni

- Oħroġ u analizza t-titlu, l-introduzzjoni u l-kontenut tad-dokument README
- Sandbox biex tmexxi demos interattivi ta 'komponenti u tiżola stili
- Ikkonverti blokki tal-kodiċi f'dokumenti f'preżentazzjonijiet interattivi
- Tqassim adattiv, wiri ta 'kolonna ta' skrin wiesa ', wiri ta' swiċċjar ta 'tab ta' skrin dejjaq

## Uża d-demo

```html
<c-webc-demo></c-webc-demo>
```

```js
import WebCDemo from "./_.js";

const el = document.querySelector("c-webc-demo");

//Inizjalizza u tikseb l-interface tal-operazzjoni
const [setMod, setMd, unMount] = WebCDemo(el);
```

## Parametri tal-interface

### `WebCDemo(el)`

Inizjalizza l-komponent.

- **parametru**
  - `el`:`HTMLElement`, kontenitur tal-komponenti.
- **Valur tar-ritorn**:`[setMod, setMd, unMount]`
  - `setMod(mod)`: Twaqqaf il-modulu demo.
  - `setMd(name, md, lang)`: Issettja l-isem tal-komponent, il-kontenut tad-dokument README u l-lingwa attwali.
  - `unMount()`: Ħatti l-komponent u naddaf is-semmiegħa tal-avveniment.

### Modulu Demo `mod` attribut

- `CSS`:`string`, il-mogħdija tal-folja tal-istil tal-komponent.
- `HTM`:`string`, il-mudell HTML tal-komponent.
- `default`:`function(root)`, funzjoni ta 'inizjalizzazzjoni tal-komponent, tirċievi kontenitur ta' l-għeruq dell.

## klassi tal-istil

### `.M`

qasam tal-kontenut.

### `.L`

Il-pannell tax-xellug fih struzzjonijiet u dokumentazzjoni.

### `.R`

Panel tal-lemin, li fih id-demo.

### `c-nav`

Bar ta 'navigazzjoni ta' skrin dejjaq.