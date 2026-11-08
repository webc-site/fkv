# Compoñente de navegación do directorio

- **Caixón sensible**: mostra a barra lateral en pantalla ancha, colárase automaticamente na pantalla estreita e coopere co caixón `c-side` e co botón de activación do título
- **Controlador multilingüe**: admite dicionario de obxectos ou funcións multilingües, responde automaticamente ao cambio de idioma
- **Subdirectorio automático**: analiza automaticamente os títulos `h1`~`h6` no contedor de destino para xerar un directorio secundario e supervisar o desprazamento desta xanela Hash.
- **Carga asíncrona**: fai clic nun elemento do directorio para obter automaticamente texto Markdown de forma asíncrona e inxéctao na área de contido, mostrando `Ing` animación de carga

## Use a demostración

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
  "use": "usar compoñente"
});
</script>
```

## Parámetros da interface

### Propiedades e métodos

- `toc.prefix`: obtén ou define a cadea do prefixo de enrutamento (como `"doc"`, a barra inclinada ao final é opcional), a tecla `README` asignarase automaticamente a unha ruta baleira (é dicir, o propio prefixo)
- `toc.li`: obtén ou establece a función de xeración de directorios en varios idiomas `(lang) => ({ key: title })`
- `toc.mdUrl`: obtén ou establece a función de xeración de URL de Markdown `(key, lang) => url`
- `toc.target`: obtén ou establece o nodo de renderización de destino asociado (como `c-md`). Se non se especifica, atopará automaticamente `c-md` / `main` / `article`
- `toc.load(key)`: activa manualmente a carga de documentos e a expansión do directorio correspondente á clave
- `toc.sync()`: volve analizar manualmente o título no nodo de destino actual, sincroniza o directorio secundario e resalta o estado