# Botóns e ventás emerxentes para cambiar o idioma da interface

- Fai clic no botón para mostrar a capa flotante de selección de idioma.
- Resalta automaticamente o idioma seleccionado actualmente.

## Use a demostración

```html
<c-i18n></c-i18n>

<script type="module">
import "webc.site/I18n.js";
// Use o seguinte código JS para supervisar os cambios de idioma
import { onLang } from "webc.site/js/i18n.js";

onLang((code, id) => {
  alert(code + ' : ' + id);
});
</script>
```

## Parámetros da interface

### Etiqueta personalizada `<c-i18n>`

Etiquetas HTML personalizadas, introducidas en JS e utilizadas en HTML.

## Xestión do estado

Use `src/js/i18n.js` para xestionar o estado global da lingua:

- **`LANG_LI`**: matriz de lista de idiomas, no formato `[[nome, ID], ...]`.
- **`lang()`**: obtén o ID de idioma actual.
- **`langCode()`**: obtén a codificación do idioma actual.
- **`langSet(id)`**: define o ID de idioma e notifica aos subscritores.
- **`onLang(func)`**: subscríbete aos cambios de idioma. Se se estableceu o idioma, a devolución de chamada activarase inmediatamente. Devolve a función de cancelar a subscrición.

## clase de estilo

### `.BtnC.lang`

botón de icona.

### `.I18n.Lg`

O deseño principal da ventá emerxente úsase para envolver as opcións de idioma.