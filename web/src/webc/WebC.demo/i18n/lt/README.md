# Pateikite ir rodykite komponentų demonstracines versijas, dokumentaciją ir diegimo instrukcijas

- Ištraukite ir išanalizuokite README dokumento pavadinimą, įvadą ir turinį
- Smėlio dėžė, skirta paleisti interaktyvias komponentų demonstracines versijas ir išskirti stilius
- Konvertuokite kodų blokus dokumentuose į interaktyvius pristatymus
- Prisitaikantis išdėstymas, plataus ekrano stulpelių ekranas, siauro ekrano skirtukų perjungimo ekranas

## Naudokite demonstracinę versiją

```html
<c-webc-demo></c-webc-demo>
```

```js
import WebCDemo from "./_.js";

const el = document.querySelector("c-webc-demo");

//Inicijuoti ir gauti operacijos sąsają
const [setMod, setMd, unMount] = WebCDemo(el);
```

## Sąsajos parametrai

### `WebCDemo(el)`

Inicijuokite komponentą.

- **parametras**
  - `el`:`HTMLElement`, komponentų talpykla.
- **Grąžinimo vertė**:`[setMod, setMd, unMount]`
  - `setMod(mod)`: nustatykite demonstracinį modulį.
  - `setMd(name, md, lang)`: nustatykite komponento pavadinimą, README dokumento turinį ir dabartinę kalbą.
  - `unMount()`: iškelkite komponentą ir išvalykite įvykių klausytoją.

### Demonstracinio modulio `mod` atributas

- `CSS`:`string`, komponento stiliaus lapo kelias.
- `HTM`:`string`, komponento HTML šablonas.
- `default`:`function(root)`, komponento inicijavimo funkcija, gauna šešėlinį šakninį konteinerį.

## stiliaus klasė

### `.M`

turinio sritis.

### `.L`

Kairiajame skydelyje yra instrukcijos ir dokumentai.

### `.R`

Dešinysis skydelis, kuriame yra demonstracinė versija.

### `c-nav`

Siauro ekrano naršymo juosta.