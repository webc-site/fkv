# Schaltflächen und Popup-Fenster zum Wechseln der Schnittstellensprachen

- Klicken Sie auf die Schaltfläche, um die schwebende Ebene für die Sprachauswahl einzublenden.
- Markiert automatisch die aktuell ausgewählte Sprache.

## Nutzen Sie die Demo

```html
<c-i18n></c-i18n>

<script type="module">
import "webc.site/I18n.js";
// Verwenden Sie den folgenden JS-Code, um Sprachänderungen zu überwachen
import { onLang } from "webc.site/js/i18n.js";

onLang((code, id) => {
  alert(code + ' : ' + id);
});
</script>
```

## Schnittstellenparameter

### Benutzerdefiniertes Tag `<c-i18n>`

Benutzerdefinierte HTML-Tags, eingeführt in JS und verwendet in HTML.

## Statusverwaltung

Verwenden Sie `src/js/i18n.js`, um den globalen Sprachstatus zu verwalten:

- **`LANG_LI`**: Sprachlistenarray im Format `[[Name, ID], ...]`.
- **`lang()`**: Rufen Sie die aktuelle Sprach-ID ab.
- **`langCode()`**: Rufen Sie die aktuelle Sprachkodierung ab.
- **`langSet(id)`**: Legen Sie die Sprach-ID fest und benachrichtigen Sie Abonnenten.
- **`onLang(func)`**: Sprachänderungen abonnieren. Wenn die Sprache eingestellt ist, wird der Rückruf sofort ausgelöst. Gibt die Abmeldefunktion zurück.

## Stilklasse

### `.BtnC.lang`

Symbolschaltfläche.

### `.I18n.Lg`

Das Hauptlayout des Popup-Fensters wird zum Umschließen der Sprachoptionen verwendet.