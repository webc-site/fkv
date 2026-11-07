# Luwas nga i-parse ug i-render ang Markdown nga teksto sa HTML.

- **Security Sanitization**: Depensa batok sa XSS attacks gamit ang lumad nga `setHTML` API o `DOMParser` cleaners
- **Pagkuha sa Content**: Awtomatikong basaha ang sulod sa teksto sa default slot atol sa pagsugod ug i-render kini
- **Gitukod-sa-typesetting**: preset nga listahan, lamesa, code block, kinutlo ug mga estilo sa kahon sa pasidaan

## Gamita ang demo

### static nga paghubad

```html
<c-md>
# titulo
Kini usa ka piraso sa Markdown content
</c-md>
```

### dinamikong buluhaton

```html
<c-md id="md"></c-md>

<script type="module">
import "webc.site/Md.js";
document.querySelector("#md").value = "#Bag-ong titulo\nBag-ong sulod";
</script>
```

## Mga parametro sa interface

Ipasa ang Markdown nga teksto diretso sa sulod sa tag (isip default slot), o i-update sa dinamikong paagi pinaagi sa `value`.

## Deskripsyon sa estilo

Ang klase sa estilo nga `.Md` awtomatik nga idugang kung ang sangkap gisugdan. Nagsuporta sa mosunod nga Markdown gipalapdan nga layout sa kahon sa pasidaan (na-trigger sa reference syntax):

- `[!NOTE]`:Abay nga impormasyon
- `[!TIP]`: Mga Sugyot/Tips
- `[!IMPORTANT]`: Importante nga pagtagad
- `[!WARNING]`: Pasidaan sa risgo
- `[!CAUTION]`: Taas kaayog risgo nga pasidaan

## mekanismo sa seguridad

Kung mag-render, tawga una ang lumad nga `setHTML` API aron luwas nga ma-inject ang HTML. Kon ang browser dili mosuporta niini, kini awtomatik nga mo-downgrade ngadto sa recursive cleaner base sa `DOMParser` ug ipatuman ang mosunod nga polisiya sa seguridad:
- Kuhaa ang dili luwas nga mga tag (pananglitan `script`, `iframe`, ug uban pa)
- Klaroha ang `on*` mga hiyas sa panghitabo
- Panalipdi batok sa mga pag-atake sa XSS pinaagi sa pagsala sa mga hiyas nga nagsugod sa peligrosong mga protocol sama sa `javascript:`, `vbscript:`, ug `data:` (gawas sa `data:image/`).