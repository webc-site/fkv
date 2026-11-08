# Tinklaraščio įrašo komponentas

- **Automatinis metų grupavimas**: automatiškai klasifikuokite metus pagal straipsnio datą ir išdėstykite juos atvirkštine tvarka pagal metus
- **Supaprastintas datos formatas**: automatiškai pašalinkite metų priešdėlį, kai rodomas, ir rodomas tik mėnuo ir diena
- **Žymėjimo santrauka ir tekstas**: integruota sąrašo santraukos peržiūra ir vieno straipsnio viso teksto atvaizdavimas
- **Gradiento trimatis pavadinimas**: Integruokite `H1Txt`, kad būtų rodoma antraštė ir išleidimo data

## Naudokite demonstracinę versiją

```html
<c-blog></c-blog>

<script type="module">
import "webc.site/Blog.js";

const blog = document.querySelector("c-blog");
blog.prefix = "/blog/";
blog.mdUrl = (key, lang) => "/doc/" + lang + "/blog/" + key + ".md";
blog._ = {
  "2026/publish": ["pavadinimas", "abstraktas", "2026-07-27"]
};

// Perjungti, kad įkeltumėte nurodytą straipsnio turinį (perduokite tuščią eilutę, kad sugrįžtumėte į sąrašą)
blog.load("2026/publish");
</script>
```

## Sąsajos parametrai

### Savybės ir metodai

- `blog.prefix`: gaukite arba nustatykite straipsnio nuorodos priešdėlį (pvz., `"/blog/"`)
- `blog.mdUrl`: gaukite arba nustatykite Markdown straipsnio adreso generavimo funkciją `(key, lang) => url`
- `blog._`: gaukite arba nustatykite straipsnių sąrašo metaduomenų žodyną `{ [key]: [title, summary, date] }`
- `blog.load(key)`: perjunkite, kad įkeltumėte nurodyto rakto straipsnio turinį, perduokite tuščią eilutę `""`, kad perjungtumėte atgal į tinklaraščių sąrašą