# Emuāra ziņas sastāvdaļa

- **Automātiska gadu grupēšana**: automātiski klasificējiet gadus pēc raksta datuma un kārtojiet tos apgrieztā secībā pēc gadiem
- **Vienkāršots datuma formāts**: automātiski noņemiet gada prefiksu, kad tiek rādīts, un tiek rādīts tikai mēnesis un diena
- **Atzīmju kopsavilkums un teksts**: iebūvēts saraksta kopsavilkuma priekšskatījums un viena raksta pilna teksta renderēšana
- **Gradienta trīsdimensiju virsraksts**: integrējiet `H1Txt`, lai parādītu virsrakstu un izdošanas datumu

## Izmantojiet demonstrāciju

```html
<c-blog></c-blog>

<script type="module">
import "webc.site/Blog.js";

const blog = document.querySelector("c-blog");
blog.prefix = "/blog/";
blog.mdUrl = (key, lang) => "/doc/" + lang + "/blog/" + key + ".md";
blog._ = {
  "2026/public": ["nosaukums", "abstract", "2026-07-27"]
};

// Pārslēgties, lai ielādētu norādīto raksta pamattekstu (nododiet tukšu virkni, lai pārslēgtos atpakaļ uz sarakstu)
blog.load("2026/publish");
</script>
```

## Interfeisa parametri

### Īpašības un metodes

- `blog.prefix`: iegūstiet vai iestatiet raksta saites prefiksu (piemēram, `"/blog/"`)
- `blog.mdUrl`: iegūstiet vai iestatiet Markdown rakstu adreses ģenerēšanas funkciju `(key, lang) => url`
- `blog._`: iegūstiet vai iestatiet rakstu saraksta metadatu vārdnīcu `{ [key]: [title, summary, date] }`
- `blog.load(key)`: pārslēdzieties, lai ielādētu norādītās atslēgas raksta saturu, nosūtiet tukšu virkni `""`, lai pārslēgtos atpakaļ uz emuāru sarakstu