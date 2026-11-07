# Compoñente de publicación de blog

- **Agrupación automática de anos**: clasifica automaticamente os anos segundo a data do artigo e organízaos en orde inversa por ano
- **Formato de data simplificado**: elimina automaticamente o prefixo do ano ao mostrar só o mes e o día
- **Resumo e texto de marcado**: vista previa do resumo da lista integrada e representación do texto completo do artigo único
- **Título tridimensional con degradado**: integre `H1Txt` para mostrar o título e a data de lanzamento

## Use a demostración

```html
<c-blog></c-blog>

<script type="module">
import "webc.site/Blog.js";

const blog = document.querySelector("c-blog");
blog.prefix = "/blog/";
blog.mdUrl = (key, lang) => "/doc/" + lang + "/blog/" + key + ".md";
blog._ = {
  "2026/publish": ["título", "abstracto", "2026-07-27"]
};

// Cambiar para cargar o corpo do artigo especificado (pase unha cadea baleira para volver á lista)
blog.load("2026/publish");
</script>
```

## Parámetros da interface

### Propiedades e métodos

- `blog.prefix`: obtén ou establece o prefixo da ligazón do artigo (como `"/blog/"`)
- `blog.mdUrl`: obtén ou define a función de xeración de enderezos de artigos de Markdown `(key, lang) => url`
- `blog._`: obtén ou establece o dicionario de metadatos da lista de artigos `{ [key]: [title, summary, date] }`
- `blog.load(key)`: cambia para cargar o contido do artigo da clave especificada, pasa unha cadea baleira `""` para volver á lista de blogs