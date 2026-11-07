# Gumbi in pojavna okna za preklapljanje med jeziki vmesnika

- Kliknite gumb, da se prikaže plavajoča plast za izbiro jezika.
- Samodejno označi trenutno izbrani jezik.

## Uporabite demo

```html
<c-i18n></c-i18n>

<script type="module">
import "webc.site/I18n.js";
// Uporabite naslednjo kodo JS za spremljanje jezikovnih sprememb
import { onLang } from "webc.site/js/i18n.js";

onLang((code, id) => {
  alert(code + ' : ' + id);
});
</script>
```

## Parametri vmesnika

### Oznaka po meri `<c-i18n>`

Oznake HTML po meri, predstavljene v JS in uporabljene v HTML.

## Upravljanje stanja

Uporabite `src/js/i18n.js` za upravljanje globalnega stanja jezika:

- **`LANG_LI`**: Niz jezikovnega seznama v obliki `[[ime, ID], ...]`.
- **`lang()`**: pridobi ID trenutnega jezika.
- **`langCode()`**: pridobi trenutno jezikovno kodiranje.
- **`langSet(id)`**: Nastavite ID jezika in obvestite naročnike.
- **`onLang(func)`**: Naročite se na jezikovne spremembe. Če je bil jezik nastavljen, se bo povratni klic takoj sprožil. Vrnite funkcijo odjave.

## stilski razred

### `.BtnC.lang`

ikona gumb.

### `.I18n.Lg`

Glavna postavitev pojavnega okna se uporablja za objem jezikovnih možnosti.