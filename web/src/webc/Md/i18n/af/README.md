# Ontleed en lewer Markdown-teks veilig na HTML.

- **Sekuriteitsreiniging**: Verdedig teen XSS-aanvalle met inheemse `setHTML` API of `DOMParser` skoonmakers
- **Inhoud onttrekking**: Lees outomaties die teksinhoud van die verstekgleuf tydens inisialisering en lewer dit weer
- ** Ingeboude tipeset**: voorafingestelde lys-, tabel-, kodeblok-, aanhalings- en waarskuwingskasstyle

## Gebruik die demo

### statiese weergawe

```html
<c-md>
# titel
Dit is 'n stukkie Markdown-inhoud
</c-md>
```

### dinamiese opdrag

```html
<c-md id="md"></c-md>

<script type="module">
import "webc.site/Md.js";
document.querySelector("#md").value = "#Nuwe titel\nNuwe inhoud";
</script>
```

## Interface parameters

Gee Markdown-teks direk binne die merker in (as die verstekgleuf), of dateer dinamies op via `value`.

## Styl beskrywing

Die `.Md` stylklas word outomaties bygevoeg wanneer die komponent geïnisialiseer word. Ondersteun die volgende Markdown uitgebreide waarskuwingsblokkie-uitleg (geaktiveer deur verwysingsintaksis):

- `[!NOTE]`：Vrieglike inligting
- `[!TIP]`: Voorstelle/wenke
- `[!IMPORTANT]`: Belangrike aandag
- `[!WARNING]`: Risikowaarskuwing
- `[!CAUTION]`: Uiters hoë risiko waarskuwing

## sekuriteitsmeganisme

Wanneer jy weergee, roep eers die oorspronklike `setHTML` API om HTML veilig in te spuit. As die blaaier dit nie ondersteun nie, sal dit outomaties afgradeer na 'n rekursiewe skoonmaker gebaseer op `DOMParser` en die volgende sekuriteitsbeleid implementeer:
- Verwyder nie-veilige merkers (bv. `script`, `iframe`, ens.)
- Vee `on*` gebeurteniskenmerke uit
- Verdedig teen XSS-aanvalle deur kenmerke te filter wat begin met gevaarlike protokolle soos `javascript:`, `vbscript:` en `data:` (behalwe `data:image/`).