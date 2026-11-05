# Pulsanti e finestre popup per cambiare la lingua dell'interfaccia

- Fare clic sul pulsante per visualizzare il livello mobile di selezione della lingua.
- Evidenzia automaticamente la lingua attualmente selezionata.

## Utilizza la demo

```html
<c-i18n></c-i18n>

<script type="module">
import "webc.site/I18n.js";
// Utilizza il seguente codice JS per monitorare le modifiche alla lingua
import { onLang } from "webc.site/js/i18n.js";

onLang((code, id) => {
  alert(code + ' : ' + id);
});
</script>
```

## Parametri dell'interfaccia

### Etichetta personalizzata `<c-i18n>`

Tag HTML personalizzati, introdotti in JS e utilizzati in HTML.

## Gestione dello stato

Utilizza `src/js/i18n.js` per gestire lo stato globale della lingua:

- **`LANG_LI`**: array dell'elenco delle lingue, nel formato `[[nome, ID], ...]`.
- **`lang()`**: ottieni l'ID della lingua corrente.
- **`langCode()`**: ottiene la codifica della lingua corrente.
- **`langSet(id)`**: imposta l'ID della lingua e avvisa gli abbonati.
- **`onLang(func)`**: iscriviti ai cambiamenti di lingua. Se la lingua è stata impostata, la richiamata verrà attivata immediatamente. Restituisce la funzione di annullamento dell'iscrizione.

## lezione di stile

### `.BtnC.lang`

pulsante icona.

### `.I18n.Lg`

Il layout principale della finestra pop-up viene utilizzato per racchiudere le opzioni della lingua.