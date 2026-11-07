# Documentación y código fuente para renderizar componentes o funciones de herramientas.

- Diseño responsivo de dos columnas, con los documentos mostrados a la izquierda y el código fuente a la derecha
- La documentación y el código fuente admiten los botones de acceso directo Editar y Codificar para saltar a la dirección de origen.
- El título y la introducción se muestran en la parte superior y admiten la traducción automática i18n.

## Usa la demostración

```html
<c-webc-js src="/src/js/dom.js" href="/doc/{code}/js/dom.md"></c-webc-js>
```

## Parámetros de propiedad

- `src`: dirección URL del código fuente.
- `href`: dirección URL de la reducción del documento, admite el uso de marcador de posición `{code}` para adaptarse al idioma actual.