# Komponent tan-Navigazzjoni tad-Direttorju

- **Kexxun li jirreaġixxi**: Uri l-istrixxa tal-ġenb fi skrin wiesa', tikkrolla awtomatikament fi skrin dejjaq u tikkoopera ma' `c-side` kexxun u buttuna ta' trigger tat-titlu
- **Sewwieq b'ħafna lingwi**: Jappoġġja dizzjunarju tal-oġġetti jew funzjonijiet b'ħafna lingwi, jirrispondi awtomatikament għall-bidla tal-lingwa
- **Subdirettorju awtomatiku**: Parse awtomatikament it-titoli `h1`~`h6` fil-kontenitur fil-mira biex tiġġenera direttorju sekondarju u tissorvelja l-iskrolljar tal-fash viewport tal-Hash.
- **Tagħbija asinkronika**: Ikklikkja fuq oġġett tad-direttorju biex takkwista awtomatikament test Markdown b'mod mhux sinkroniku u injettah fiż-żona tal-kontenut, filwaqt li turi `Ing` animazzjoni tat-tagħbija

## Uża d-demo

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
  "use": "uża komponent"
});
</script>
```

## Parametri tal-interface

### Proprjetajiet u metodi

- `toc.prefix`: Ikseb jew issettja s-sekwenza tal-prefiss tar-routing (bħal `"doc"`, it-trailing slash hija fakultattiva), iċ-ċavetta `README` se tiġi mmappjata awtomatikament għal mogħdija vojta (jiġifieri l-prefiss innifsu)
- `toc.li`: Ikseb jew issettja l-funzjoni ta' ġenerazzjoni tad-direttorju b'diversi lingwi `(lang) => ({ key: title })`
- `toc.mdUrl`: Ikseb jew issettja l-funzjoni tal-ġenerazzjoni tal-URL Markdown `(key, lang) => url`
- `toc.target`: Ikseb jew issettja n-nodu tar-rendi tal-mira assoċjat (bħal `c-md`). Jekk mhux speċifikat, awtomatikament issib `c-md` / `main` / `article`
- `toc.load(key)`: Ikkawża manwalment it-tagħbija tad-dokumenti u l-espansjoni tad-direttorju li tikkorrispondi għaċ-ċavetta
- `toc.sync()`: Erġa' teżamina manwalment it-titlu fin-nodu fil-mira attwali, issinkronizza d-direttorju sekondarju u enfasizza l-istatus