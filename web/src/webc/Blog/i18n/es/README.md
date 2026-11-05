# Componente de publicación de blog

- **Agrupación automática de años**: clasifica los años automáticamente según la fecha del artículo y los organiza en orden inverso por año.
- **Formato de fecha simplificado**: elimina automáticamente el prefijo de año al mostrar y solo muestra el mes y el día.
- **Resumen y texto de Markdown**: vista previa del resumen de la lista incorporada y representación del texto completo de un solo artículo
- **Título tridimensional degradado**: integra `H1Txt` para mostrar el título y la fecha de lanzamiento.

## Usa la demostración

```html
<c-blog></c-blog>

<script type="module">
import "webc.site/Blog.js";

const blog = document.querySelector("c-blog");
blog.prefix = "/blog/";
blog.mdUrl = (key, lang) => "/doc/" + lang + "/blog/" + key + ".md";
blog._ = {
  "2026/publicar": ["título", "resumen", "2026-07-27"]
};

// Cambiar para cargar el cuerpo del artículo especificado (pase una cadena vacía para volver a la lista)
blog.load("2026/publish");
</script>
```

## Parámetros de interfaz

### Propiedades y métodos

- `blog.prefix`: obtiene o establece el prefijo del enlace del artículo (como `"/blog/"`)
- `blog.mdUrl`: obtiene o configura la función de generación de direcciones de artículos de Markdown `(key, lang) => url`
- `blog._`: Obtener o configurar el diccionario de metadatos de la lista de artículos `{ [key]: [title, summary, date] }`
- `blog.load(key)`: cambia para cargar el contenido del artículo de la clave especificada, pasa una cadena vacía `""` para volver a la lista de blogs.