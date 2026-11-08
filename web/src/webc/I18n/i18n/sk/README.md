# Tlačidlá a kontextové okná na prepínanie jazykov rozhrania

- Kliknutím na tlačidlo zobrazíte plávajúcu vrstvu výberu jazyka.
- Automaticky zvýrazniť aktuálne vybraný jazyk.

## Použite demo

```html
<c-i18n></c-i18n>

<script type="module">
import "webc.site/I18n.js";
// Na sledovanie zmien jazyka použite nasledujúci kód JS
import { onLang } from "webc.site/js/i18n.js";

onLang((code, id) => {
  alert(code + ' : ' + id);
});
</script>
```

## Parametre rozhrania

### Vlastná značka `<c-i18n>`

Vlastné značky HTML zavedené v JS a používané v HTML.

## Správa stavu

Pomocou `src/js/i18n.js` spravujte globálny stav jazyka:

- **`LANG_LI`**: Pole zoznamu jazykov vo formáte `[[názov, ID], ...]`.
- **`lang()`**: Získajte aktuálne ID jazyka.
- **`langCode()`**: Získajte aktuálne kódovanie jazyka.
- **`langSet(id)`**: Nastavte ID jazyka a upozornite predplatiteľov.
- **`onLang(func)`**: Prihláste sa na odber jazykových zmien. Ak bol jazyk nastavený, spätné volanie sa spustí okamžite. Vráťte funkciu odhlásenia.

## štýlová trieda

### `.BtnC.lang`

tlačidlo ikony.

### `.I18n.Lg`

Hlavné rozloženie kontextového okna sa používa na zabalenie jazykových možností.