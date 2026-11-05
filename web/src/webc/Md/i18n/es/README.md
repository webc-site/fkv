# Analice y renderice de forma segura texto Markdown a HTML.

- **Desinfección de seguridad**: defiéndase contra ataques XSS utilizando la API `setHTML` nativa o los limpiadores `DOMParser`
- **Extracción de contenido**: lee automáticamente el contenido de texto de la ranura predeterminada durante la inicialización y lo representa.
- **Composición tipográfica incorporada**: estilos de lista preestablecida, tabla, bloque de código, cita y cuadro de advertencia

## Usa la demostración

### renderizado estático

```html
<c-md>
# título
Esta es una parte del contenido de Markdown.
</c-md>
```

### asignación dinámica

```html
<c-md id="md"></c-md>

<script type="module">
import "webc.site/Md.js";
document.querySelector("#md").value = "#Nuevo título\nNuevo contenido";
</script>
```

## Parámetros de interfaz

Pase el texto de Markdown directamente dentro de la etiqueta (como espacio predeterminado) o actualícelo dinámicamente a través de `value`.

## Descripción del estilo

La clase de estilo `.Md` se agrega automáticamente cuando se inicializa el componente. Admite el siguiente diseño de cuadro de advertencia extendido de Markdown (activado por sintaxis de referencia):

- `[!NOTE]`：Información rápida
- `[!TIP]`: Sugerencias/Consejos
- `[!IMPORTANT]`: Atención importante
- `[!WARNING]`: Advertencia de riesgo
- `[!CAUTION]`: Advertencia de riesgo extremadamente alto

## mecanismo de seguridad

Al renderizar, llame primero a la API nativa `setHTML` para inyectar HTML de forma segura. Si el navegador no lo admite, cambiará automáticamente a un limpiador recursivo basado en `DOMParser` e implementará la siguiente política de seguridad:
- Elimine etiquetas no seguras (por ejemplo, `script`, `iframe`, etc.)
- Borrar atributos de evento `on*`
- Defiéndase de los ataques XSS filtrando atributos que comiencen con protocolos peligrosos como `javascript:`, `vbscript:` y `data:` (excepto `data:image/`).