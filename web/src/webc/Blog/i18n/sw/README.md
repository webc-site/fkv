# Sehemu ya chapisho la blogi

- **Kupanga mwaka otomatiki**: Panga miaka kiotomatiki kulingana na tarehe ya makala, na uipange kwa mpangilio wa kinyume baada ya mwaka
- **Muundo wa Tarehe Uliorahisishwa**: Ondoa kiambishi awali cha mwaka kiotomatiki unapoonyeshwa, na uonyeshe mwezi na siku pekee
- **Muhtasari na maandishi**: Onyesho la kuchungulia la muhtasari wa orodha iliyojumuishwa na uwasilishaji wa maandishi kamili wa kifungu kimoja
- **Kichwa cha gradient tatu-dimensional**: Integrate `H1Txt` ili kuonyesha kichwa cha habari na tarehe ya kutolewa

## Tumia onyesho

```html
<c-blog></c-blog>

<script type="module">
import "webc.site/Blog.js";

const blog = document.querySelector("c-blog");
blog.prefix = "/blog/";
blog.mdUrl = (key, lang) => "/doc/" + lang + "/blog/" + key + ".md";
blog._ = {
  "2026/publish": ["title", "abstract", "2026-07-27"]
};

// Badili ili kupakia mwili wa makala maalum (pitisha kamba tupu ili kurudi kwenye orodha)
blog.load("2026/publish");
</script>
```

## Vigezo vya interface

### Mali na mbinu

- `blog.prefix`: Pata au weka kiambishi cha kiungo cha makala (kama vile `"/blog/"`)
- `blog.mdUrl`: Pata au weka kipengele cha kutengeneza anwani ya makala ya Markdown `(key, lang) => url`
- `blog._`: Pata au weka kamusi ya metadata ya orodha ya makala `{ [key]: [title, summary, date] }`
- `blog.load(key)`: Badili ili kupakia maudhui ya makala ya ufunguo uliobainishwa, pitisha kamba tupu `""` ili kurudi kwenye orodha ya blogu