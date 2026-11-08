# Blog post component

- **Automatic year grouping**: Automatically classify years according to article date, and arrange them in reverse order by year
- **Simplified Date Format**: Automatically remove the year prefix when displaying, and only display the month and day
- **Markdown summary and text**: Built-in list summary preview and single article complete text rendering
- **Gradient three-dimensional title**: Integrate `H1Txt` to display the headline and release date

## Use the demo

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

// Switch to load the specified article body (pass an empty string to switch back to the list)
blog.load("2026/publish");
</script>
```

## Interface parameters

### Properties and methods

- `blog.prefix`: Get or set the article link prefix (such as `"/blog/"`)
- `blog.mdUrl`: Get or set the Markdown article address generation function `(key, lang) => url`
- `blog._`: Get or set the article list metadata dictionary `{ [key]: [title, summary, date] }`
- `blog.load(key)`: Switch to load the article content of the specified key, pass an empty string `""` to switch back to the blog list