# Hluti bloggfærslu

- **Sjálfvirk árflokkun**: Flokkaðu ár sjálfkrafa í samræmi við greinardagsetningu og raðaðu þeim í öfuga röð eftir árum
- **Einfaldað dagsetningarsnið**: Fjarlægðu ártalsforskeytið sjálfkrafa þegar það er sýnt og birtir aðeins mánuð og dag
- **Yfirlit yfir merkingar og texti**: Innbyggt forskoðun listasamantektar og fullkominn textaflutningur á einni grein
- **Títill þrívíddar gráðugur**: Samþætta `H1Txt` til að birta fyrirsögn og útgáfudag

## Notaðu kynninguna

```html
<c-blog></c-blog>

<script type="module">
import "webc.site/Blog.js";

const blog = document.querySelector("c-blog");
blog.prefix = "/blog/";
blog.mdUrl = (key, lang) => "/doc/" + lang + "/blog/" + key + ".md";
blog._ = {
  "2026/publish": ["titill", "ágrip", "2026-07-27"]
};

// Skiptu til að hlaða tilgreindum greinarhluta (slepptu tómum streng til að skipta aftur í listann)
blog.load("2026/publish");
</script>
```

## Viðmótsbreytur

### Eiginleikar og aðferðir

- `blog.prefix`: Fáðu eða stilltu forskeyti greinartengils (eins og `"/blog/"`)
- `blog.mdUrl`: Fáðu eða stilltu Markdown-grein heimilisfang kynslóð aðgerð `(key, lang) => url`
- `blog._`: Fáðu eða stilltu lýsigagnaorðabók greinalista `{ [key]: [title, summary, date] }`
- `blog.load(key)`: Skiptu til að hlaða greinarinnihaldi tilgreinds lykils, sendu tóman streng `""` til að skipta aftur í blogglistann