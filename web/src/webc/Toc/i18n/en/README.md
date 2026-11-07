# Directory Navigation Component

- **Responsive drawer**: Display sidebar in wide screen, automatically collapse in narrow screen and cooperate with `c-side` drawer and title trigger button
- **Multi-language driver**: Supports object dictionary or multi-language functions, automatically responds to language switching
- **Automatic subdirectory**: Automatically parse the `h1`~`h6` titles in the target container to generate a secondary directory and monitor the Hash viewport scrolling highlight.
- **Asynchronous loading**: Click on a directory item to automatically obtain Markdown text asynchronously and inject it into the content area, displaying `Ing` loading animation

## Use the demo

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
  "use": "use component"
});
</script>
```

## Interface parameters

### Properties and methods

- `toc.prefix`: Get or set the routing prefix string (such as `"doc"`, the trailing slash is optional), the `README` key will be automatically mapped to an empty path (i.e. the prefix itself)
- `toc.li`: Get or set the directory multi-language generation function `(lang) => ({ key: title })`
- `toc.mdUrl`: Get or set the Markdown URL generation function `(key, lang) => url`
- `toc.target`: Get or set the associated target rendering node (such as `c-md`). If not specified, it will automatically find `c-md` / `main` / `article`
- `toc.load(key)`: Manually trigger document loading and directory expansion corresponding to key
- `toc.sync()`: Manually re-parse the title in the current target node, synchronize the secondary directory and highlight status