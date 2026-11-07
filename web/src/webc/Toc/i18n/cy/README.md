# Cydran Llywio Cyfeiriadur

- **Drôr ymatebol**: Arddangos bar ochr mewn sgrin lydan, cwympo'n awtomatig yn y sgrin gul a chydweithredu â `c-side` drôr a botwm sbardun teitl
- ** Gyrrwr aml-iaith**: Yn cefnogi swyddogaethau geiriadur gwrthrychau neu aml-iaith, yn ymateb yn awtomatig i newid iaith
- **Is-gyfeiriadur awtomatig**: Dosrannu'r teitlau `h1`~ `h6` yn awtomatig yn y cynhwysydd targed i gynhyrchu cyfeiriadur eilaidd a monitro uchafbwynt sgrolio porth gwylio Hash.
- **Llwytho anghydamserol**: Cliciwch ar eitem cyfeiriadur i gael testun Markdown yn anghydamserol yn awtomatig a'i chwistrellu i'r ardal gynnwys, gan ddangos `Ing` animeiddiad llwytho

## Defnyddiwch y demo

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
  "use": "defnyddio cydran"
});
</script>
```

## Paramedrau rhyngwyneb

### Priodweddau a dulliau

- `toc.prefix`: Cael neu osod y llinyn rhagddodiad llwybro (megis `"doc"`, mae'r slaes llusgo yn ddewisol), bydd yr allwedd `README` yn cael ei mapio'n awtomatig i lwybr gwag (h.y. y rhagddodiad ei hun)
- `toc.li`: Cael neu osod y cyfeiriadur swyddogaeth cynhyrchu aml-iaith `(lang) => ({ key: title })`
- `toc.mdUrl`: Cael neu osod y swyddogaeth cynhyrchu URL Markdown `(key, lang) => url`
- `toc.target`: Cael neu osod y nod rendro targed cysylltiedig (fel `c-md`). Os na chaiff ei nodi, bydd yn dod o hyd i `c-md` / `main` / `article` yn awtomatig
- `toc.load(key)`: Sbardun llwytho dogfen â llaw ac ehangu cyfeiriadur sy'n cyfateb i'r allwedd
- `toc.sync()`: Ail-ddosrannu'r teitl â llaw yn y nod targed cyfredol, cydamseru'r cyfeiriadur eilaidd a statws amlygu