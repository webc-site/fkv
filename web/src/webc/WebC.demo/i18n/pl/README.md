# Renderuj i wyświetlaj demonstracje komponentów, dokumentację i instrukcje instalacji

- Wyodrębnij i przeanalizuj tytuł, wprowadzenie i treść dokumentu README
- Sandbox do uruchamiania interaktywnych demonstracji komponentów i izolowania stylów
- Konwertuj bloki kodu w dokumentach na interaktywne prezentacje
- Układ adaptacyjny, wyświetlanie kolumn na szerokim ekranie, wyświetlanie przełączania zakładek na wąskim ekranie

## Skorzystaj z wersji demonstracyjnej

```html
<c-webc-demo></c-webc-demo>
```

```js
import WebCDemo from "./_.js";

const el = document.querySelector("c-webc-demo");

//Zainicjuj i uzyskaj interfejs operacyjny
const [setMod, setMd, unMount] = WebCDemo(el);
```

## Parametry interfejsu

### `WebCDemo(el)`

Zainicjuj komponent.

- **parametr**
  - `el`:`HTMLElement`, kontener komponentów.
- **Wartość zwracana**:`[setMod, setMd, unMount]`
  - `setMod(mod)`: Skonfiguruj moduł demonstracyjny.
  - `setMd(name, md, lang)`: Ustaw nazwę komponentu, treść dokumentu README i bieżący język.
  - `unMount()`: Wyładuj komponent i wyczyść detektor zdarzeń.

### Atrybut modułu demonstracyjnego `mod`

- `CSS`:`string`, ścieżka arkusza stylów komponentu.
- `HTM`:`string`, szablon HTML komponentu.
- `default`:`function(root)`, funkcja inicjowania komponentu, odbiera kontener główny w tle.

## klasa stylu

### `.M`

obszar treści.

### `.L`

Lewy panel zawiera instrukcje i dokumentację.

### `.R`

Prawy panel zawierający wersję demonstracyjną.

### `c-nav`

Wąski pasek nawigacji na ekranie.