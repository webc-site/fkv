# Analizoni dhe jepni në mënyrë të sigurt tekstin Markdown në HTML.

- **Sanitizimi i sigurisë**: Mbroni kundër sulmeve XSS duke përdorur pastrues amtare `setHTML` ose `DOMParser`
- **Nxjerrja e përmbajtjes**: Lexoni automatikisht përmbajtjen e tekstit të slotit të paracaktuar gjatë inicializimit dhe jepni atë
- **Vendosja e integruar**: lista e paracaktuar, tabela, blloku i kodeve, kutitë e kutisë së kuotave dhe paralajmërimi

## Përdorni demonstrimin

### pasqyrim statik

```html
<c-md>
# titullin
Kjo është një pjesë e përmbajtjes së Markdown
</c-md>
```

### detyrë dinamike

```html
<c-md id="md"></c-md>

<script type="module">
import "webc.site/Md.js";
document.querySelector("#md").value = "#Titulli i ri\nPërmbajtje e re";
</script>
```

## Parametrat e ndërfaqes

Kaloni tekstin Markdown direkt brenda etiketës (si vend i paracaktuar), ose përditësoni në mënyrë dinamike nëpërmjet `value`.

## Përshkrimi i stilit

Klasa e stilit `.Md` shtohet automatikisht kur komponenti inicializohet. Mbështet paraqitjen e mëposhtme të kutisë së paralajmërimit të zgjeruar Markdown (shkaktuar nga sintaksa e referencës):

- `[!NOTE]`：Informacion i shpejtë
- `[!TIP]`: Sugjerime/Këshilla
- `[!IMPORTANT]`: Vëmendje e rëndësishme
- `[!WARNING]`: Paralajmërim rreziku
- `[!CAUTION]`: Paralajmërim me rrezik jashtëzakonisht të lartë

## mekanizmi i sigurisë

Kur jepni, telefononi fillimisht API-në vendase `setHTML` për të injektuar në mënyrë të sigurt HTML. Nëse shfletuesi nuk e mbështet atë, ai automatikisht do të zbresë në një pastrues rekurziv bazuar në `DOMParser` dhe do të zbatojë politikën e mëposhtme të sigurisë:
- Hiq etiketat jo të sigurta (p.sh. `script`, `iframe`, etj.)
- Pastro atributet e ngjarjes `on*`
- Mbroni kundër sulmeve XSS duke filtruar atributet duke filluar me protokolle të rrezikshme si `javascript:`, `vbscript:` dhe `data:` (përveç `data:image/`).