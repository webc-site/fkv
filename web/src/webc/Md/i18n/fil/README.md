# Ligtas na i-parse at i-render ang Markdown text sa HTML.

- **Security Sanitization**: Ipagtanggol laban sa mga pag-atake ng XSS gamit ang mga native na `setHTML` API o `DOMParser` na tagapaglinis
- **Pagkuha ng Nilalaman**: Awtomatikong basahin ang nilalaman ng teksto ng default na puwang sa panahon ng pagsisimula at i-render ito
- **Built-in na typesetting**: preset list, table, code block, quote at mga istilo ng warning box

## Gamitin ang demo

### static na pag-render

```html
<c-md>
# pamagat
Ito ay isang piraso ng Markdown na nilalaman
</c-md>
```

### dinamikong takdang-aralin

```html
<c-md id="md"></c-md>

<script type="module">
import "webc.site/Md.js";
document.querySelector("#md").value = "#Bagong pamagat\nBagong nilalaman";
</script>
```

## Mga parameter ng interface

Direktang ipasa ang Markdown text sa loob ng tag (bilang default na slot), o dynamic na mag-update sa pamamagitan ng `value`.

## Paglalarawan ng istilo

Awtomatikong idinaragdag ang klase ng estilo ng `.Md` kapag nasimulan ang bahagi. Sinusuportahan ang sumusunod na Markdown extended warning box layout (na-trigger ng reference syntax):

- `[!NOTE]`：Prompt na impormasyon
- `[!TIP]`: Mga Mungkahi/Tip
- `[!IMPORTANT]`: Mahalagang pansin
- `[!WARNING]`: Babala sa panganib
- `[!CAUTION]`: Babala sa napakataas na panganib

## mekanismo ng seguridad

Kapag nagre-render, tawagan muna ang native na `setHTML` API para ligtas na mag-inject ng HTML. Kung hindi ito sinusuportahan ng browser, awtomatiko itong magda-downgrade sa isang recursive cleaner batay sa `DOMParser` at ipapatupad ang sumusunod na patakaran sa seguridad:
- Alisin ang mga hindi ligtas na tag (hal. `script`, `iframe`, atbp.)
- I-clear ang `on*` mga katangian ng kaganapan
- Ipagtanggol laban sa mga pag-atake ng XSS sa pamamagitan ng pag-filter ng mga attribute na nagsisimula sa mga mapanganib na protocol gaya ng `javascript:`, `vbscript:`, at `data:` (maliban sa `data:image/`).