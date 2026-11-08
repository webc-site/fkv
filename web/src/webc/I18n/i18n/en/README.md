# Buttons and pop-up windows for switching interface languages

- Click the button to pop up the language selection floating layer.
- Automatically highlight the currently selected language.

## Use the demo

```html
<c-i18n></c-i18n>

<script type="module">
import "webc.site/I18n.js";
// Use the following JS code to monitor language changes
import { onLang } from "webc.site/js/i18n.js";

onLang((code, id) => {
  alert(code + ' : ' + id);
});
</script>
```

## Interface parameters

### Custom tag `<c-i18n>`

Custom HTML tags, introduced in JS and used in HTML.

## Status management

Use `src/js/i18n.js` to manage global language status:

- **`LANG_LI`**: Language list array, in the format of `[[name, ID], ...]`.
- **`lang()`**: Get the current language ID.
- **`langCode()`**: Get the current language encoding.
- **`langSet(id)`**: Set the language ID and notify subscribers.
- **`onLang(func)`**: Subscribe to language changes. If the language has been set, the callback will be triggered immediately. Return the unsubscribe function.

## Style Class

### `.BtnC.lang`

icon button.

### `.I18n.Lg`

The main layout of the pop-up window is used to wrap the language options.