# Component ng Directory Navigation

- **Responsive drawer**: Ipakita ang sidebar sa malawak na screen, awtomatikong i-collapse sa makitid na screen at makipagtulungan sa `c-side` drawer at title trigger button
- **Multi-language driver**: Sinusuportahan ang object dictionary o multi-language functions, awtomatikong tumutugon sa pagpapalit ng wika
- **Awtomatikong subdirectory**: Awtomatikong i-parse ang `h1`~`h6` na mga pamagat sa target na lalagyan upang bumuo ng pangalawang direktoryo at subaybayan ang Hash viewport scrolling highlight.
- **Asynchronous na paglo-load**: Mag-click sa isang item sa direktoryo upang awtomatikong makakuha ng Markdown na teksto nang asynchronous at ipasok ito sa lugar ng nilalaman, na nagpapakita ng `Ing` naglo-load ng animation

## Gamitin ang demo

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

## Mga parameter ng interface

### Mga katangian at pamamaraan

- `toc.prefix`: Kunin o itakda ang routing prefix string (gaya ng `"doc"`, ang trailing slash ay opsyonal), ang `README` key ay awtomatikong imamapa sa isang walang laman na path (ibig sabihin, ang prefix mismo)
- `toc.li`: Kunin o itakda ang directory na multi-language generation function `(lang) => ({ key: title })`
- `toc.mdUrl`: Kunin o itakda ang Markdown URL generation function `(key, lang) => url`
- `toc.target`: Kunin o itakda ang nauugnay na target na rendering node (gaya ng `c-md`). Kung hindi tinukoy, awtomatiko nitong mahahanap ang `c-md` / `main` / `article`
- `toc.load(key)`: Manu-manong i-trigger ang pag-load ng dokumento at pagpapalawak ng direktoryo na naaayon sa key
- `toc.sync()`: Manu-manong muling i-parse ang pamagat sa kasalukuyang target na node, i-synchronize ang pangalawang direktoryo at i-highlight ang katayuan