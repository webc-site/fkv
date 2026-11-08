# Renderirajte i prikažite demonstracije komponenti, dokumentaciju i upute za instalaciju

- Izdvojite i raščlanite naslov, uvod i sadržaj README dokumenta
- Sandbox za pokretanje interaktivnih demonstracija komponenti i izolaciju stilova
- Pretvorite blokove koda u dokumentima u interaktivne prezentacije
- Prilagodljivi raspored, široki ekran sa kolonama, uski ekran za promenu kartica

## Koristite demo

```html
<c-webc-demo></c-webc-demo>
```

```js
import WebCDemo from "./_.js";

const el = document.querySelector("c-webc-demo");

//Inicijalizirati i dobiti operativni interfejs
const [setMod, setMd, unMount] = WebCDemo(el);
```

## Parametri interfejsa

### `WebCDemo(el)`

Inicijalizirajte komponentu.

- **parametar**
  - `el`:`HTMLElement`, kontejner komponente.
- **Povratna vrijednost**:`[setMod, setMd, unMount]`
  - `setMod(mod)`: Postavite demo modul.
  - `setMd(name, md, lang)`: Postavite naziv komponente, sadržaj README dokumenta i trenutni jezik.
  - `unMount()`: Učitajte komponentu i očistite slušatelj događaja.

### Demo modul `mod` atribut

- `CSS`:`string`, putanja liste stilova komponente.
- `HTM`:`string`, HTML šablon komponente.
- `default`:`function(root)`, funkcija inicijalizacije komponente, prima sjeni korijenski kontejner.

## stilska klasa

### `.M`

područje sadržaja.

### `.L`

Lijevi panel sadrži upute i dokumentaciju.

### `.R`

Desni panel, koji sadrži demo.

### `c-nav`

Uska traka za navigaciju na ekranu.