# Tlačítka a vyskakovací okna pro přepínání jazyků rozhraní

- Klepnutím na tlačítko zobrazíte plovoucí vrstvu výběru jazyka.
- Automaticky zvýraznit aktuálně vybraný jazyk.

## Použijte demo

```html
<c-i18n></c-i18n>

<script type="module">
import "webc.site/I18n.js";
// Ke sledování změn jazyka použijte následující kód JS
import { onLang } from "webc.site/js/i18n.js";

onLang((code, id) => {
  alert(code + ' : ' + id);
});
</script>
```

## Parametry rozhraní

### Vlastní značka `<c-i18n>`

Vlastní značky HTML zavedené v JS a používané v HTML.

## Správa stavu

Pomocí `src/js/i18n.js` můžete spravovat globální stav jazyka:

- **`LANG_LI`**: Pole seznamu jazyků ve formátu `[[jméno, ID], ...]`.
- **`lang()`**: Získejte aktuální ID jazyka.
- **`langCode()`**: Získejte aktuální kódování jazyka.
- **`langSet(id)`**: Nastavte ID jazyka a upozorněte předplatitele.
- **`onLang(func)`**: Přihlaste se k odběru jazykových změn. Pokud byl jazyk nastaven, zpětné volání se spustí okamžitě. Vraťte funkci odhlášení.

## stylová třída

### `.BtnC.lang`

tlačítko ikony.

### `.I18n.Lg`

Hlavní rozložení vyskakovacího okna se používá k zabalení jazykových možností.