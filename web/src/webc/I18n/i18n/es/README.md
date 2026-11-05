# Botones y ventanas emergentes para cambiar el idioma de la interfaz

- Haga clic en el botón para que aparezca la capa flotante de selección de idioma.
- Resalta automáticamente el idioma seleccionado actualmente.

## Usa la demostración

```html
<c-i18n></c-i18n>

<script type="module">
import "webc.site/I18n.js";
// Utilice el siguiente código JS para monitorear los cambios de idioma
import { onLang } from "webc.site/js/i18n.js";

onLang((code, id) => {
  alert(code + ' : ' + id);
});
</script>
```

## Parámetros de interfaz

### Etiqueta personalizada `<c-i18n>`

Etiquetas HTML personalizadas, introducidas en JS y utilizadas en HTML.

## Gestión de estado

Utilice `src/js/i18n.js` para administrar el estado global del idioma:

- **`LANG_LI`**: Matriz de lista de idiomas, en el formato de `[[nombre, ID], ...]`.
- **`lang()`**: obtiene el ID del idioma actual.
- **`langCode()`**: obtiene la codificación del idioma actual.
- **`langSet(id)`**: establece el ID de idioma y notifica a los suscriptores.
- **`onLang(func)`**: Suscríbete para recibir cambios de idioma. Si se ha configurado el idioma, la devolución de llamada se activará inmediatamente. Devuelve la función de cancelación de suscripción.

## clase de estilo

### `.BtnC.lang`

botón de icono.

### `.I18n.Lg`

El diseño principal de la ventana emergente se utiliza para ajustar las opciones de idioma.