# Bahagi ng post sa blog

- **Awtomatikong pagpapangkat ng taon**: Awtomatikong uriin ang mga taon ayon sa petsa ng artikulo, at ayusin ang mga ito sa baligtad na pagkakasunud-sunod ayon sa taon
- **Pinasimpleng Format ng Petsa**: Awtomatikong alisin ang prefix ng taon kapag ipinapakita, at ipapakita lang ang buwan at araw
- **Buod ng markdown at teksto**: Built-in na preview ng buod ng listahan at kumpletong pag-render ng teksto ng isang artikulo
- **Gradient three-dimensional na pamagat**: Isama ang `H1Txt` upang ipakita ang headline at petsa ng paglabas

## Gamitin ang demo

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

// Lumipat upang i-load ang tinukoy na katawan ng artikulo (magpasa ng walang laman na string upang bumalik sa listahan)
blog.load("2026/publish");
</script>
```

## Mga parameter ng interface

### Mga katangian at pamamaraan

- `blog.prefix`: Kunin o itakda ang prefix ng link ng artikulo (gaya ng `"/blog/"`)
- `blog.mdUrl`: Kunin o itakda ang Markdown article address generation function `(key, lang) => url`
- `blog._`: Kunin o itakda ang diksyunaryo ng metadata ng listahan ng artikulo `{ [key]: [title, summary, date] }`
- `blog.load(key)`: Lumipat upang i-load ang nilalaman ng artikulo ng tinukoy na key, magpasa ng walang laman na string `""` upang bumalik sa listahan ng blog