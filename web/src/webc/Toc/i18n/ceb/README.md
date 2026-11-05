# Direktoryo nga Navigation Component

- **Responsive drawer**: Ipakita ang sidebar sa lapad nga screen, awtomatik nga mahugno sa pig-ot nga screen ug mokooperar sa `c-side` drawer ug title trigger button
- ** Multi-language driver **: Nagsuporta sa object dictionary o multi-language functions, awtomatikong motubag sa switching language
- **Awtomatikong subdirektoryo**: Awtomatikong i-parse ang `h1`~`h6` nga mga titulo sa target nga sudlanan aron makamugna og ikaduhang direktoryo ug mamonitor ang Hash viewport scrolling highlight.
- **Asynchronous loading**: Pag-klik sa usa ka direktoryo nga butang aron awtomatik nga makakuha og Markdown nga teksto nga asynchronous ug i-inject kini ngadto sa content area, magpakita sa `Ing` loading animation

## Gamita ang demo

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
  "use": "gamiton ang component"
});
</script>
```

## Mga parametro sa interface

### Mga kabtangan ug pamaagi

- `toc.prefix`: Kuhaa o i-set ang routing prefix string (sama sa `"doc"`, opsyonal ang trailing slash), ang `README` nga yawe awtomatik nga ma-mapa ngadto sa walay sulod nga dalan (ie ang prefix mismo)
- `toc.li`: Kuhaa o itakda ang direktoryo nga multi-language generation function `(lang) => ({ key: title })`
- `toc.mdUrl`: Kuhaa o itakda ang Markdown URL generation function `(key, lang) => url`
- `toc.target`: Kuhaa o itakda ang kaubang target rendering node (sama sa `c-md`). Kung wala gitino, kini awtomatik nga makit-an ang `c-md` / `main` / `article`
- `toc.load(key)`: Manwal nga mag-trigger sa pagkarga sa dokumento ug pagpalapad sa direktoryo nga katumbas sa yawe
- `toc.sync()`: Manwal nga i-parse pag-usab ang titulo sa kasamtangang target node, i-synchronize ang ikaduhang direktoryo ug i-highlight ang status