# Errendatu eta bistaratu osagaien demoak, dokumentazioa eta instalazio-argibideak

- Atera eta analizatu IRAKURRI dokumentuaren izenburua, sarrera eta edukia
- Sandbox osagaien demo interaktiboak exekutatzeko eta estiloak isolatzeko
- Bihurtu dokumentuetako kode-blokeak aurkezpen interaktiboetan
- Diseinu moldagarria, pantaila zabaleko zutabeen bistaratzea, pantaila estuaren fitxa aldatzeko pantaila

## Erabili demoa

```html
<c-webc-demo></c-webc-demo>
```

```js
import WebCDemo from "./_.js";

const el = document.querySelector("c-webc-demo");

//Hasi eta lortu eragiketa-interfazea
const [setMod, setMd, unMount] = WebCDemo(el);
```

## Interfazearen parametroak

### `WebCDemo(el)`

Hasieratu osagaia.

- **parametroa**
  - `el`:`HTMLElement`, osagaien edukiontzia.
- **Itzuli balioa**:`[setMod, setMd, unMount]`
  - `setMod(mod)`: konfiguratu demo modulua.
  - `setMd(name, md, lang)`: ezarri osagaiaren izena, IRAKURRI dokumentuaren edukia eta uneko hizkuntza.
  - `unMount()`: deskargatu osagaia eta garbitu gertaeren entzulea.

### Demo moduluaren `mod` atributua

- `CSS`:`string`, osagaiaren estilo-orriaren bidea.
- `HTM`:`string`, osagaiaren HTML txantiloia.
- `default`:`function(root)`, osagaiak hasieratzeko funtzioa, itzal erroko edukiontzia jasotzen du.

## estilo klasea

### `.M`

eduki-eremua.

### `.L`

Ezkerreko panelak argibideak eta dokumentazioa ditu.

### `.R`

Eskuineko panela, demoa duena.

### `c-nav`

Pantaila estua nabigazio-barra.