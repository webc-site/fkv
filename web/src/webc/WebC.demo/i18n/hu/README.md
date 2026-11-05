# Rendereljen és jelenítsen meg komponens bemutatókat, dokumentációt és telepítési utasításokat

- Bontsa ki és elemezze a README dokumentum címét, bevezetését és tartalmát
- Sandbox az összetevők interaktív demóinak futtatásához és a stílusok elkülönítéséhez
- A dokumentumokban lévő kódblokkokat interaktív bemutatókká alakíthatja
- Adaptív elrendezés, széles képernyős oszlopmegjelenítés, keskeny képernyős fülváltás

## Használd a demót

```html
<c-webc-demo></c-webc-demo>
```

```js
import WebCDemo from "./_.js";

const el = document.querySelector("c-webc-demo");

//Inicializálja és szerezze be a műveleti felületet
const [setMod, setMd, unMount] = WebCDemo(el);
```

## Interfész paraméterei

### `WebCDemo(el)`

Inicializálja az összetevőt.

- **paraméter**
  - `el`:`HTMLElement`, alkatrésztároló.
- **Visszatérési érték**:`[setMod, setMd, unMount]`
  - `setMod(mod)`: A bemutató modul beállítása.
  - `setMd(name, md, lang)`: Állítsa be az összetevő nevét, a README dokumentum tartalmát és az aktuális nyelvet.
  - `unMount()`: Töltse ki az összetevőt, és tisztítsa meg az eseményfigyelőt.

### Demo modul `mod` attribútuma

- `CSS`:`string`, az összetevő stíluslap elérési útja.
- `HTM`:`string`, az összetevő HTML-sablonja.
- `default`:`function(root)`, komponens inicializálási funkció, árnyékgyökér tárolót kap.

## stílus osztály

### `.M`

tartalmi terület.

### `.L`

A bal oldali panel utasításokat és dokumentációt tartalmaz.

### `.R`

A demót tartalmazó jobb oldali panel.

### `c-nav`

Keskeny képernyős navigációs sáv.