# Renderujte a zobrazujte ukázky komponent, dokumentaci a pokyny k instalaci

- Extrahujte a analyzujte název, úvod a obsah dokumentu README
- Sandbox pro spouštění interaktivních ukázek komponent a izolování stylů
- Převeďte bloky kódu v dokumentech do interaktivních prezentací
- Adaptivní rozvržení, širokoúhlý sloupcový displej, úzký displej přepínání karet

## Použijte demo

```html
<c-webc-demo></c-webc-demo>
```

```js
import WebCDemo from "./_.js";

const el = document.querySelector("c-webc-demo");

//Inicializovat a získat operační rozhraní
const [setMod, setMd, unMount] = WebCDemo(el);
```

## Parametry rozhraní

### `WebCDemo(el)`

Inicializujte komponentu.

- **parametr**
  - `el`:`HTMLElement`, kontejner komponent.
- **Vrácená hodnota**:`[setMod, setMd, unMount]`
  - `setMod(mod)`: Nastavte demo modul.
  - `setMd(name, md, lang)`: Nastavte název komponenty, obsah dokumentu README a aktuální jazyk.
  - `unMount()`: Uvolněte komponentu a vyčistěte posluchač událostí.

### Atribut ukázkového modulu `mod`

- `CSS`:`string`, cesta šablony stylů komponenty.
- `HTM`:`string`, šablona HTML komponenty.
- `default`:`function(root)`, funkce inicializace komponenty, přijímá stínový kořenový kontejner.

## stylová třída

### `.M`

oblast obsahu.

### `.L`

Levý panel obsahuje pokyny a dokumentaci.

### `.R`

Pravý panel obsahující ukázku.

### `c-nav`

Navigační lišta na úzké obrazovce.