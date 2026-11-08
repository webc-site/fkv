# Gerðu og sýndu sýnikennslu íhluta, skjöl og uppsetningarleiðbeiningar

- Dragðu út og greindu titil, inngang og innihald README skjalsins
- Sandkassi til að keyra gagnvirkar kynningar á íhlutum og einangra stíla
- Umbreyttu kóðablokkum í skjölum í gagnvirkar kynningar
- Aðlagandi skipulag, breiður skjár dálkaskjár, þröngur skjáflipaskiptaskjár

## Notaðu kynninguna

```html
<c-webc-demo></c-webc-demo>
```

```js
import WebCDemo from "./_.js";

const el = document.querySelector("c-webc-demo");

//Frumstilla og fáðu aðgerðaviðmótið
const [setMod, setMd, unMount] = WebCDemo(el);
```

## Viðmótsbreytur

### `WebCDemo(el)`

Frumstilla íhlutinn.

- **færibreyta**
  - `el`:`HTMLElement`, íhlutaílát.
- **Ávöxtunargildi**:`[setMod, setMd, unMount]`
  - `setMod(mod)`: Settu upp kynningareininguna.
  - `setMd(name, md, lang)`: Stilltu heiti íhluta, innihald README skjalsins og núverandi tungumál.
  - `unMount()`: Fjarlægðu íhlutinn og hreinsaðu viðburðahlustarann.

### Sýningareining `mod` eigind

- `CSS`:`string`, stílblaðsslóð íhlutsins.
- `HTM`:`string`, HTML sniðmát íhlutans.
- `default`:`function(root)`, frumstillingaraðgerð íhluta, tekur á móti skuggarótaríláti.

## stílaflokkur

### `.M`

efnissvæði.

### `.L`

Vinstra spjaldið inniheldur leiðbeiningar og skjöl.

### `.R`

Hægra spjaldið, sem inniheldur kynninguna.

### `c-nav`

Yfirlitsstika á þröngum skjá.