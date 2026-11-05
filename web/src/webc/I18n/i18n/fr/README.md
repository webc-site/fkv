# Boutons et fenêtres contextuelles pour changer de langue d'interface

- Cliquez sur le bouton pour faire apparaître le calque flottant de sélection de langue.
- Mettez automatiquement en surbrillance la langue actuellement sélectionnée.

## Utilisez la démo

```html
<c-i18n></c-i18n>

<script type="module">
import "webc.site/I18n.js";
// Utilisez le code JS suivant pour surveiller les changements de langue
import { onLang } from "webc.site/js/i18n.js";

onLang((code, id) => {
  alert(code + ' : ' + id);
});
</script>
```

## Paramètres d'interface

### Balise personnalisée `<c-i18n>`

Balises HTML personnalisées, introduites dans JS et utilisées en HTML.

## Gestion des statuts

Utilisez `src/js/i18n.js` pour gérer l'état global de la langue :

- **`LANG_LI`** : Tableau de liste de langues, au format `[[name, ID], ...]`.
- **`lang()`** : obtenez l'ID de langue actuel.
- **`langCode()`** : obtenez l'encodage de la langue actuelle.
- **`langSet(id)`** : définissez l'ID de langue et informez les abonnés.
- **`onLang(func)`** : Abonnez-vous aux changements de langue. Si la langue a été définie, le rappel sera déclenché immédiatement. Renvoie la fonction de désabonnement.

## cours de style

### `.BtnC.lang`

bouton icône.

### `.I18n.Lg`

La disposition principale de la fenêtre contextuelle est utilisée pour envelopper les options de langue.