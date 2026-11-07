# Mygtukai ir iššokantys langai sąsajos kalboms perjungti

- Spustelėkite mygtuką, kad iššoktumėte kalbos pasirinkimo slankiojantį sluoksnį.
- Automatiškai paryškinkite šiuo metu pasirinktą kalbą.

## Naudokite demonstracinę versiją

```html
<c-i18n></c-i18n>

<script type="module">
import "webc.site/I18n.js";
// Kalbos pasikeitimams stebėti naudokite šį JS kodą
import { onLang } from "webc.site/js/i18n.js";

onLang((code, id) => {
  alert(code + ' : ' + id);
});
</script>
```

## Sąsajos parametrai

### Tinkinta žyma `<c-i18n>`

Tinkintos HTML žymos, įvestos JS ir naudojamos HTML.

## Būsenos valdymas

Naudokite `src/js/i18n.js` visuotinei kalbos būsenai valdyti:

- **`LANG_LI`**: kalbų sąrašo masyvas `[[vardas, ID], ...]` formatu.
- **`lang()`**: gaukite dabartinės kalbos ID.
- **`langCode()`**: gaukite dabartinės kalbos kodavimą.
- **`langSet(id)`**: nustatykite kalbos ID ir praneškite prenumeratoriams.
- **`onLang(func)`**: užsiprenumeruokite kalbos pakeitimus. Jei kalba buvo nustatyta, skambutis bus nedelsiant suaktyvintas. Grąžinti prenumeratos atsisakymo funkciją.

## stiliaus klasė

### `.BtnC.lang`

piktogramos mygtukas.

### `.I18n.Lg`

Pagrindinis iškylančiojo lango išdėstymas naudojamas kalbos parinktims apvynioti.