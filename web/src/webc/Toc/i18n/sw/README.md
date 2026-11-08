# Kipengele cha Urambazaji wa Saraka

- **Droo inayojibu**: Onyesha utepe katika skrini pana, ukunje kiotomatiki kwenye skrini nyembamba na ushirikiane na `c-side` kitufe cha droo na kichochezi cha mada.
- **Kiendeshaji cha lugha nyingi**: Hutumia kamusi ya kitu au vitendaji vya lugha nyingi, hujibu kiotomatiki ubadilishanaji wa lugha
- **Kanda ndogo ya kiotomatiki**: Changanua kiotomatiki `h1`~`h6` mada katika kontena lengwa ili kuunda saraka ya pili na kufuatilia uangaziaji wa kusogeza wa kituo cha kutazama cha Hash.
- **Upakiaji usiolingana**: Bofya kwenye kipengee cha saraka ili kupata maandishi ya Markdown kiotomatiki kwa usawa na kuyaingiza kwenye eneo la maudhui, kuonyesha `Ing` uhuishaji wa kupakia.

## Tumia onyesho

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
  "use": "tumia kipengele"
});
</script>
```

## Vigezo vya interface

### Mali na mbinu

- `toc.prefix`: Pata au weka mfuatano wa kiambishi awali cha uelekezaji (kama vile `"doc"`, kufyeka trailing ni hiari), `README` ufunguo utachorwa kiotomatiki kwa njia tupu (yaani kiambishi awali chenyewe)
- `toc.li`: Pata au weka kitendakazi cha uundaji wa lugha nyingi `(lang) => ({ key: title })`
- `toc.mdUrl`: Pata au weka kitendakazi cha kutengeneza URL ya Markdown `(key, lang) => url`
- `toc.target`: Pata au weka nodi ya uwasilishaji inayolengwa (kama vile `c-md`). Ikiwa haijabainishwa, itapata kiotomatiki `c-md` / `main` / `article`
- `toc.load(key)`: Anzisha upakiaji wa hati wewe mwenyewe na upanuzi wa saraka unaolingana na ufunguo
- `toc.sync()`: Changanua upya kichwa wewe mwenyewe katika nodi lengwa ya sasa, sawazisha saraka ya pili na uangazie hali.