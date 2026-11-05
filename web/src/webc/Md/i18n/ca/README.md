# Analitzeu i renderitzeu el text Markdown de manera segura a HTML.

- **Desinfecció de seguretat**: defensa dels atacs XSS amb `setHTML` API nadius o `DOMParser` netejadors
- **Extracció de contingut**: llegiu automàticament el contingut de text de l'espai predeterminat durant la inicialització i representa-lo
- **Escriptura integrada**: estils de llista preestablerta, taula, bloc de codi, cites i quadres d'avís

## Utilitzeu la demostració

### renderització estàtica

```html
<c-md>
# títol
Aquest és un contingut de Markdown
</c-md>
```

### assignació dinàmica

```html
<c-md id="md"></c-md>

<script type="module">
import "webc.site/Md.js";
document.querySelector("#md").value = "#Nou títol\nNou contingut";
</script>
```

## Paràmetres de la interfície

Passeu el text Markdown directament dins de l'etiqueta (com a espai predeterminat) o actualitzeu-lo dinàmicament mitjançant `value`.

## Descripció de l'estil

La classe d'estil `.Md` s'afegeix automàticament quan s'inicia el component. Admet el següent disseny de quadre d'advertència de Markdown (activat per la sintaxi de referència):

- `[!NOTE]`：Informació ràpida
- `[!TIP]`: Suggeriments/Consells
- `[!IMPORTANT]`: atenció important
- `[!WARNING]`: advertència de risc
- `[!CAUTION]`: advertència de risc extremadament alt

## mecanisme de seguretat

En renderitzar, truqueu primer a l'API nativa `setHTML` per injectar HTML de manera segura. Si el navegador no l'admet, canviarà automàticament a un netejador recursiu basat en `DOMParser` i implementarà la política de seguretat següent:
- Traieu les etiquetes no segures (p. ex., `script`, `iframe`, etc.)
- Esborra `on*` atributs d'esdeveniment
- Defensa contra els atacs XSS filtrant els atributs que comencen amb protocols perillosos com ara `javascript:`, `vbscript:` i `data:` (excepte `data:image/`).