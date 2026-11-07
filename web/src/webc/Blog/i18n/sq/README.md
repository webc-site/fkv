# Komponenti i postimit në blog

- **Grupimi automatik i viteve**: Klasifikoni automatikisht vitet sipas datës së artikullit dhe rregulloni ato në rend të kundërt sipas vitit
- **Formati i thjeshtuar i datës**: Hiq automatikisht prefiksin e vitit kur shfaqet dhe shfaq vetëm muajin dhe ditën
- **Përmbledhja e shënimit dhe teksti **: Pamja paraprake e përmbledhjes së listës së integruar dhe përkthimi i tekstit të plotë të një artikulli të vetëm
- **Titulli tre-dimensional i gradientit**: Integroni `H1Txt` për të shfaqur titullin dhe datën e publikimit

## Përdorni demonstrimin

```html
<c-blog></c-blog>

<script type="module">
import "webc.site/Blog.js";

const blog = document.querySelector("c-blog");
blog.prefix = "/blog/";
blog.mdUrl = (key, lang) => "/doc/" + lang + "/blog/" + key + ".md";
blog._ = {
  "2026/publikoj": ["titull", "abstrakt", "2026-07-27"]
};

// Kalo për të ngarkuar trupin e specifikuar të artikullit (kalo një varg bosh për të kaluar përsëri në listë)
blog.load("2026/publish");
</script>
```

## Parametrat e ndërfaqes

### Vetitë dhe metodat

- `blog.prefix`: Merrni ose vendosni prefiksin e lidhjes së artikullit (si p.sh. `"/blog/"`)
- `blog.mdUrl`: Merrni ose vendosni funksionin e gjenerimit të adresës së artikullit Markdown `(key, lang) => url`
- `blog._`: Merr ose cakto fjalorin e meta të dhënave të listës së artikujve `{ [key]: [title, summary, date] }`
- `blog.load(key)`: Kalo për të ngarkuar përmbajtjen e artikullit të çelësit të specifikuar, kaloni një varg bosh `""` për t'u kthyer në listën e blogut