# Renderirajte i prikažite demonstracije komponenti, dokumentaciju i upute za instalaciju

- Ekstrahirajte i analizirajte naslov, uvod i sadržaj dokumenta README
- Sandbox za pokretanje interaktivnih demonstracija komponenti i izdvajanje stilova
- Pretvorite blokove koda u dokumentima u interaktivne prezentacije
- Prilagodljiv izgled, široki zaslonski prikaz stupaca, uski zaslonski prikaz s promjenom kartica

## Koristite demo

```html
<c-webc-demo></c-webc-demo>
```

```js
import WebCDemo from "./_.js";

const el = document.querySelector("c-webc-demo");

//Inicijaliziraj i dobij operativno sučelje
const [setMod, setMd, unMount] = WebCDemo(el);
```

## Parametri sučelja

### `WebCDemo(el)`

Inicijalizirajte komponentu.

- **parametar**
  - `el`:`HTMLElement`, spremnik komponente.
- **Povratna vrijednost**:`[setMod, setMd, unMount]`
  - `setMod(mod)`: Postavite demo modul.
  - `setMd(name, md, lang)`: Postavite naziv komponente, sadržaj dokumenta README i trenutni jezik.
  - `unMount()`: Učitajte komponentu i očistite slušatelja događaja.

### Atribut `mod` demo modula

- `CSS`:`string`, staza stilskog lista komponente.
- `HTM`:`string`, HTML predložak komponente.
- `default`:`function(root)`, funkcija inicijalizacije komponente, prima korijenski spremnik sjene.

## stilska klasa

### `.M`

područje sadržaja.

### `.L`

Lijeva ploča sadrži upute i dokumentaciju.

### `.R`

Desna ploča koja sadrži demo.

### `c-nav`

Navigacijska traka uskog zaslona.