# Butonat dhe dritaret kërcyese për ndërrimin e gjuhëve të ndërfaqes

- Klikoni butonin për të shfaqur shtresën lundruese të përzgjedhjes së gjuhës.
- Theksoni automatikisht gjuhën e zgjedhur aktualisht.

## Përdorni demonstrimin

```html
<c-i18n></c-i18n>

<script type="module">
import "webc.site/I18n.js";
// Përdor kodin e mëposhtëm JS për të monitoruar ndryshimet e gjuhës
import { onLang } from "webc.site/js/i18n.js";

onLang((code, id) => {
  alert(code + ' : ' + id);
});
</script>
```

## Parametrat e ndërfaqes

### Etiketa e personalizuar `<c-i18n>`

Etiketat e personalizuara HTML, të prezantuara në JS dhe të përdorura në HTML.

## Menaxhimi i statusit

Përdorni `src/js/i18n.js` për të menaxhuar statusin global të gjuhës:

- **`LANG_LI`**: grup i listës së gjuhëve, në formatin `[[emri, ID], ...]`.
- ***`lang()`**: Merrni ID-në e gjuhës aktuale.
- ***`langCode()`**: Merrni kodimin aktual të gjuhës.
- ***`langSet(id)`**: Vendosni ID-në e gjuhës dhe njoftoni pajtimtarët.
- ***`onLang(func)`**: Abonohu ​​për ndryshimet gjuhësore. Nëse gjuha është caktuar, kthimi i telefonatës do të aktivizohet menjëherë. Ktheni funksionin e çregjistrimit.

## klasë stili

### `.BtnC.lang`

butonin e ikonës.

### `.I18n.Lg`

Paraqitja kryesore e dritares pop-up përdoret për të mbështjellë opsionet e gjuhës.