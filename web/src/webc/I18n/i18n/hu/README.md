# Gombok és felugró ablakok az interfész nyelvének váltásához

- Kattintson a gombra a nyelvválasztó lebegő réteg megjelenítéséhez.
- Az aktuálisan kiválasztott nyelv automatikus kiemelése.

## Használd a demót

```html
<c-i18n></c-i18n>

<script type="module">
import "webc.site/I18n.js";
// Használja a következő JS-kódot a nyelvi változások figyeléséhez
import { onLang } from "webc.site/js/i18n.js";

onLang((code, id) => {
  alert(code + ' : ' + id);
});
</script>
```

## Interfész paraméterei

### Egyéni címke `<c-i18n>`

Egyéni HTML-címkék, JS-ben bevezetve és HTML-ben.

## Állapotkezelés

A `src/js/i18n.js` segítségével kezelheti a globális nyelvi állapotot:

- **`LANG_LI`**: Nyelvi listatömb, `[[név, azonosító], ...]` formátumban.
- **`lang()`**: Az aktuális nyelvi azonosító lekérése.
- **`langCode()`**: Az aktuális nyelvi kódolás lekérése.
- **`langSet(id)`**: Állítsa be a nyelvi azonosítót, és értesítse az előfizetőket.
- **`onLang(func)`**: Feliratkozás a nyelvi változásokra. Ha a nyelv be van állítva, a visszahívás azonnal elindul. A leiratkozás funkció visszaállítása.

## stílus osztály

### `.BtnC.lang`

ikon gomb.

### `.I18n.Lg`

A felugró ablak fő elrendezése a nyelvi beállítások tördelésére szolgál.