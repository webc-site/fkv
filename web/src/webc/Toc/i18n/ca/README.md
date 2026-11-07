# Component de navegació de directoris

- **Calaaix sensible**: mostra la barra lateral en pantalla ampla, replega automàticament a la pantalla estreta i col·labora amb el `c-side` calaix i el botó d'activació del títol
- **Controlador multi-idioma**: admet diccionari d'objectes o funcions multi-idioma, respon automàticament al canvi d'idioma
- **Subdirectori automàtic**: analitzeu automàticament els `h1`~`h6` títols del contenidor de destinació per generar un directori secundari i supervisar el desplaçament de la finestra gràfica Hash.
- **Càrrega asíncrona**: feu clic a un element de directori per obtenir automàticament text de Markdown de manera asíncrona i injectar-lo a l'àrea de contingut, mostrant `Ing` animació de càrrega

## Utilitzeu la demostració

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
  "use": "utilitza el component"
});
</script>
```

## Paràmetres de la interfície

### Propietats i mètodes

- `toc.prefix`: obteniu o definiu la cadena del prefix d'encaminament (com ara `"doc"`, la barra inclinada final és opcional), la clau `README` s'assignarà automàticament a un camí buit (és a dir, el prefix en si).
- `toc.li`: obteniu o definiu la funció de generació multi-idioma del directori `(lang) => ({ key: title })`
- `toc.mdUrl`: obteniu o configureu la funció de generació d'URL de Markdown `(key, lang) => url`
- `toc.target`: obteniu o definiu el node de representació de destinació associat (com ara `c-md`). Si no s'especifica, trobarà automàticament `c-md` / `main` / `article`
- `toc.load(key)`: activa manualment la càrrega de documents i l'expansió del directori corresponent a la clau
- `toc.sync()`: Torneu a analitzar manualment el títol al node de destinació actual, sincronitzeu el directori secundari i ressalteu l'estat