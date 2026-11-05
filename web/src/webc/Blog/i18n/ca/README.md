# Component de publicació del bloc

- **Agrupació automàtica d'anys**: classifica automàticament els anys segons la data de l'article i ordena'ls en ordre invers per any
- **Format de data simplificat**: elimina automàticament el prefix de l'any quan es mostra i només mostra el mes i el dia
- **Resum i text de marcatge**: vista prèvia del resum de la llista integrada i representació del text complet d'un sol article
- **Títol tridimensional degradat**: integreu `H1Txt` per mostrar el títol i la data de llançament

## Utilitzeu la demostració

```html
<c-blog></c-blog>

<script type="module">
import "webc.site/Blog.js";

const blog = document.querySelector("c-blog");
blog.prefix = "/blog/";
blog.mdUrl = (key, lang) => "/doc/" + lang + "/blog/" + key + ".md";
blog._ = {
  "2026/publish": ["títol", "abstract", "2026-07-27"]
};

// Canvia per carregar el cos de l'article especificat (passa una cadena buida per tornar a la llista)
blog.load("2026/publish");
</script>
```

## Paràmetres de la interfície

### Propietats i mètodes

- `blog.prefix`: obteniu o definiu el prefix d'enllaç de l'article (com ara `"/blog/"`)
- `blog.mdUrl`: obteniu o configureu la funció de generació d'adreces d'article Markdown `(key, lang) => url`
- `blog._`: obteniu o configureu el diccionari de metadades de la llista d'articles `{ [key]: [title, summary, date] }`
- `blog.load(key)`: canvia per carregar el contingut de l'article de la clau especificada, passa una cadena buida `""` per tornar a la llista de blocs