# Renderitzar i mostrar demostracions de components, documentació i instruccions d'instal·lació

- Extraieu i analitzeu el títol, la introducció i el contingut del document README
- Sandbox per executar demostracions interactives de components i aïllar estils
- Converteix blocs de codi dels documents en presentacions interactives
- Disseny adaptatiu, visualització de columna de pantalla ampla, visualització de canvi de pestanyes de pantalla estreta

## Utilitzeu la demostració

```html
<c-webc-demo></c-webc-demo>
```

```js
import WebCDemo from "./_.js";

const el = document.querySelector("c-webc-demo");

//Inicialitzar i obtenir la interfície d'operació
const [setMod, setMd, unMount] = WebCDemo(el);
```

## Paràmetres de la interfície

### `WebCDemo(el)`

Inicialitzar el component.

- **paràmetre**
  - `el`:`HTMLElement`, contenidor de components.
- **Valor de retorn**:`[setMod, setMd, unMount]`
  - `setMod(mod)`: configureu el mòdul de demostració.
  - `setMd(name, md, lang)`: defineix el nom del component, el contingut del document README i l'idioma actual.
  - `unMount()`: descarregueu el component i netegeu l'escolta d'esdeveniments.

### Atribut del mòdul de demostració `mod`

- `CSS`:`string`, la ruta del full d'estil del component.
- `HTM`:`string`, la plantilla HTML del component.
- `default`:`function(root)`, funció d'inicialització de components, rep el contenidor arrel ombra.

## classe d'estil

### `.M`

àrea de contingut.

### `.L`

El tauler esquerre conté instruccions i documentació.

### `.R`

Tauler dret, que conté la demostració.

### `c-nav`

Barra de navegació de pantalla estreta.