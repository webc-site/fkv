# Botons i finestres emergents per canviar els idiomes de la interfície

- Feu clic al botó per obrir la capa flotant de selecció d'idioma.
- Ressalteu automàticament l'idioma seleccionat actualment.

## Utilitzeu la demostració

```html
<c-i18n></c-i18n>

<script type="module">
import "webc.site/I18n.js";
// Utilitzeu el codi JS següent per supervisar els canvis d'idioma
import { onLang } from "webc.site/js/i18n.js";

onLang((code, id) => {
  alert(code + ' : ' + id);
});
</script>
```

## Paràmetres de la interfície

### Etiqueta personalitzada `<c-i18n>`

Etiquetes HTML personalitzades, introduïdes a JS i utilitzades a HTML.

## Gestió de l'estat

Utilitzeu `src/js/i18n.js` per gestionar l'estat global de l'idioma:

- **`LANG_LI`**: matriu de llista d'idiomes, en el format `[[nom, ID], ...]`.
- **`lang()`**: obteniu l'identificador de l'idioma actual.
- **`langCode()`**: obteniu la codificació de l'idioma actual.
- **`langSet(id)`**: defineix l'identificador de l'idioma i notifica'ls als subscriptors.
- **`onLang(func)`**: subscriu-te als canvis d'idioma. Si s'ha configurat l'idioma, la devolució de trucada s'activarà immediatament. Retorna la funció de baixa.

## classe d'estil

### `.BtnC.lang`

botó d'icona.

### `.I18n.Lg`

El disseny principal de la finestra emergent s'utilitza per embolicar les opcions d'idioma.