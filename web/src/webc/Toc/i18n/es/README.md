# Componente de navegación de directorio

- **Cajón responsivo**: muestra la barra lateral en pantalla ancha, contrae automáticamente en pantalla estrecha y coopera con el `c-side` cajón y el botón de activación de título.
- **Controlador multilingüe**: Admite diccionario de objetos o funciones multilingües, responde automáticamente al cambio de idioma
- **Subdirectorio automático**: analiza automáticamente los títulos `h1`~`h6` en el contenedor de destino para generar un directorio secundario y monitorear el resaltado de desplazamiento de la ventana gráfica Hash.
- **Carga asincrónica**: haga clic en un elemento del directorio para obtener automáticamente el texto de Markdown de forma asincrónica e inyectarlo en el área de contenido, mostrando `Ing` animación de carga.

## Usa la demostración

```html
<c-toc></c-toc>
<c-md></c-md>

<script type="module">
import "webc.site/Toc.js";

const toc = document.querySelector("c-toc");
toc.target = document.querySelector("c-md");
toc.prefix = "doc";
toc.mdUrl = (key, lang) => "/doc/" + lang + "/" + key + ".md";
toc.li = (lang) => ({
  "README": "WebC.site",
  "use": "usar componente"
});
</script>
```

## Parámetros de interfaz

### Propiedades y métodos

- `toc.prefix`: obtiene o establece la cadena de prefijo de ruta (como `"doc"`, la barra diagonal final es opcional), la clave `README` se asignará automáticamente a una ruta vacía (es decir, el prefijo mismo)
- `toc.li`: Obtiene o configura la función de generación de directorios en varios idiomas `(lang) => ({ key: title })`
- `toc.mdUrl`: obtiene o configura la función de generación de URL de Markdown `(key, lang) => url`
- `toc.target`: obtiene o establece el nodo de representación de destino asociado (como `c-md`). Si no se especifica, encontrará automáticamente `c-md` / `main` / `article`
- `toc.load(key)`: activa manualmente la carga del documento y la expansión del directorio correspondiente a la clave
- `toc.sync()`: vuelva a analizar manualmente el título en el nodo de destino actual, sincronice el directorio secundario y resalte el estado