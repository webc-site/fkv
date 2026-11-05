# Przyciski i wyskakujące okienka do zmiany języka interfejsu

- Kliknij przycisk, aby wyświetlić pływającą warstwę wyboru języka.
- Automatycznie podświetl aktualnie wybrany język.

## Skorzystaj z wersji demonstracyjnej

```html
<c-i18n></c-i18n>

<script type="module">
import "webc.site/I18n.js";
// Użyj poniższego kodu JS, aby monitorować zmiany językowe
import { onLang } from "webc.site/js/i18n.js";

onLang((code, id) => {
  alert(code + ' : ' + id);
});
</script>
```

## Parametry interfejsu

### Tag niestandardowy `<c-i18n>`

Niestandardowe tagi HTML, wprowadzone w JS i używane w HTML.

## Zarządzanie statusem

Użyj `src/js/i18n.js` do zarządzania globalnym stanem języka:

- **`LANG_LI`**: Tablica listy języków w formacie `[[nazwa, identyfikator], ...]`.
- **`lang()`**: Pobierz bieżący identyfikator języka.
- **`langCode()`**: Pobierz bieżące kodowanie języka.
- **`langSet(id)`**: Ustaw identyfikator języka i powiadom subskrybentów.
- **`onLang(func)`**: Subskrybuj zmiany językowe. Jeśli język został ustawiony, wywołanie zwrotne zostanie natychmiast uruchomione. Zwróć funkcję anulowania subskrypcji.

## klasa stylu

### `.BtnC.lang`

ikona przycisku.

### `.I18n.Lg`

Główny układ wyskakującego okna służy do zawijania opcji językowych.