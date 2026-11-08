# Lewer en vertoon komponentdemo's, dokumentasie en installasie-instruksies

- Onttrek en ontleed die titel, inleiding en inhoud van die README-dokument
- Sandbox om interaktiewe demonstrasies van komponente uit te voer en style te isoleer
- Omskep kodeblokke in dokumente in interaktiewe aanbiedings
- Aanpasbare uitleg, wyeskermkolomvertoning, smal skermoortjie-wisselvertoning

## Gebruik die demo

```html
<c-webc-demo></c-webc-demo>
```

```js
import WebCDemo from "./_.js";

const el = document.querySelector("c-webc-demo");

//Inisialiseer en verkry die operasie-koppelvlak
const [setMod, setMd, unMount] = WebCDemo(el);
```

## Interface parameters

### `WebCDemo(el)`

Inisialiseer die komponent.

- **parameter**
  - `el`:`HTMLElement`, komponenthouer.
- **Opbrengswaarde**:`[setMod, setMd, unMount]`
  - `setMod(mod)`: Stel die demo-module op.
  - `setMd(name, md, lang)`: Stel die komponentnaam, README-dokumentinhoud en huidige taal in.
  - `unMount()`: Laai die komponent af en maak die gebeurtenisluisteraar skoon.

### Demo module `mod` kenmerk

- `CSS`:`string`, die stylbladpad van die komponent.
- `HTM`:`string`, die komponent se HTML-sjabloon.
- `default`:`function(root)`, komponent-inisialiseringsfunksie, ontvang skaduwortelhouer.

## styl klas

### `.M`

inhoud area.

### `.L`

Die linkerpaneel bevat instruksies en dokumentasie.

### `.R`

Regspaneel, wat die demo bevat.

### `c-nav`

Smal skerm navigasiebalk.