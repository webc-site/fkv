# Knapper og pop op-vinduer til at skifte grænsefladesprog

- Klik på knappen for at åbne det flydende lag for valg af sprog.
- Fremhæv automatisk det aktuelt valgte sprog.

## Brug demoen

```html
<c-i18n></c-i18n>

<script type="module">
import "webc.site/I18n.js";
// Brug følgende JS-kode til at overvåge sprogændringer
import { onLang } from "webc.site/js/i18n.js";

onLang((code, id) => {
  alert(code + ' : ' + id);
});
</script>
```

## Interface parametre

### Tilpasset tag `<c-i18n>`

Brugerdefinerede HTML-tags, introduceret i JS og brugt i HTML.

## Statusstyring

Brug `src/js/i18n.js` til at administrere global sprogstatus:

- **`LANG_LI`**: Sproglistearray i formatet `[[navn, ID], ...]`.
- **`lang()`**: Få det aktuelle sprog-id.
- **`langCode()`**: Hent den aktuelle sprogkodning.
- **`langSet(id)`**: Indstil sprog-id'et og underret abonnenter.
- **`onLang(func)`**: Abonner på sprogændringer. Hvis sproget er indstillet, udløses tilbagekaldet med det samme. Returner afmeldingsfunktionen.

## stil klasse

### `.BtnC.lang`

ikon knap.

### `.I18n.Lg`

Hovedlayoutet af pop op-vinduet bruges til at ombryde sprogindstillingerne.