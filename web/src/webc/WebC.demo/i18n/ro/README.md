# Redați și afișați demonstrații ale componentelor, documentație și instrucțiuni de instalare

- Extrageți și analizați titlul, introducerea și conținutul documentului README
- Sandbox pentru a rula demonstrații interactive ale componentelor și pentru a izola stiluri
- Transformați blocurile de cod din documente în prezentări interactive
- Aspect adaptiv, afișare coloană pe ecran lat, afișare de comutare a filelor de ecran îngust

## Utilizați demonstrația

```html
<c-webc-demo></c-webc-demo>
```

```js
import WebCDemo from "./_.js";

const el = document.querySelector("c-webc-demo");

//Inițializați și obțineți interfața de operare
const [setMod, setMd, unMount] = WebCDemo(el);
```

## Parametrii interfeței

### `WebCDemo(el)`

Inițializați componenta.

- **parametru**
  - `el`:`HTMLElement`, container de componente.
- **Valoare returnată**:`[setMod, setMd, unMount]`
  - `setMod(mod)`: configurați modulul demonstrativ.
  - `setMd(name, md, lang)`: setați numele componentei, conținutul documentului README și limba curentă.
  - `unMount()`: descărcați componenta și curățați ascultătorul de evenimente.

### Atributul modulului demonstrativ `mod`

- `CSS`:`string`, calea pentru foaia de stil a componentei.
- `HTM`:`string`, șablonul HTML al componentei.
- `default`:`function(root)`, funcția de inițializare a componentelor, primește containerul rădăcină umbră.

## clasa de stil

### `.M`

zona de continut.

### `.L`

Panoul din stânga conține instrucțiuni și documentație.

### `.R`

Panoul din dreapta, care conține demonstrația.

### `c-nav`

Bara de navigare cu ecran îngust.