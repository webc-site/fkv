# Componenta de navigare în director

- **Sertar receptiv**: Afișează bara laterală pe ecran lat, restrânge automat pe ecranul îngust și cooperează cu sertarul `c-side` și butonul de declanșare a titlului
- **Driver în mai multe limbi**: acceptă dicționar de obiecte sau funcții în mai multe limbi, răspunde automat la schimbarea limbii
- **Subdirector automat**: analizați automat titlurile `h1`~`h6` din containerul țintă pentru a genera un director secundar și pentru a monitoriza evidențierea derulării feței de vizualizare Hash.
- **Încărcare asincronă**: faceți clic pe un element de director pentru a obține automat textul Markdown în mod asincron și injectați-l în zona de conținut, afișând animația de încărcare `Ing`

## Utilizați demonstrația

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
  "use": "utilizați componenta"
});
</script>
```

## Parametrii interfeței

### Proprietăți și metode

- `toc.prefix`: obțineți sau setați șirul de prefix de rutare (cum ar fi `"doc"`, bara oblică finală este opțională), tasta `README` va fi mapată automat la o cale goală (adică prefixul în sine)
- `toc.li`: Obțineți sau setați funcția de generare a directorului în mai multe limbi `(lang) => ({ key: title })`
- `toc.mdUrl`: obțineți sau setați funcția de generare a adreselor URL Markdown `(key, lang) => url`
- `toc.target`: obțineți sau setați nodul de redare țintă asociat (cum ar fi `c-md`). Dacă nu este specificat, va găsi automat `c-md` / `main` / `article`
- `toc.load(key)`: declanșează manual încărcarea documentului și extinderea directorului corespunzătoare cheii
- `toc.sync()`: re-analizează manual titlul în nodul țintă curent, sincronizează directorul secundar și evidențiază starea