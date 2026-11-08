# Parse kanthi aman lan render teks Markdown menyang HTML.

- **Sanitisasi Keamanan**: Mbela serangan XSS nggunakake pembersih `setHTML` API utawa `DOMParser` asli
- ** Ekstraksi Konten **: Waca kanthi otomatis isi teks saka slot gawan nalika initialization lan nerjemahake
- **Setelan huruf internal**: dhaptar prasetel, tabel, blok kode, kutipan lan gaya kothak bebaya

## Gunakake demo

### rendering statis

```html
<c-md>
# judhul
Iki minangka potongan konten Markdown
</c-md>
```

### tugas dinamis

```html
<c-md id="md"></c-md>

<script type="module">
import "webc.site/Md.js";
document.querySelector("#md").value = "#Judul anyar\nKonten anyar";
</script>
```

## Paramèter antarmuka

Lebokake teks Markdown langsung ing tag (minangka slot standar), utawa nganyari kanthi dinamis liwat `value`.

## Katrangan gaya

Kelas gaya `.Md` ditambahake kanthi otomatis nalika komponen diwiwiti. Ndhukung tata letak kothak peringatan Markdown ing ngisor iki (dipicu dening sintaks referensi):

- `[!NOTE]`: Informasi cepet
- `[!TIP]`: Saran/Tips
- `[!IMPORTANT]`: Wigati penting
- `[!WARNING]`: Peringatan risiko
- `[!CAUTION]`: Peringatan beresiko dhuwur banget

## mekanisme keamanan

Nalika rendering, nelpon API asli `setHTML` kanggo nyuntikake HTML kanthi aman. Yen browser ora ndhukung, bakal kanthi otomatis downgrade menyang rekursif resik adhedhasar `DOMParser` lan ngleksanakake kabijakan keamanan ing ngisor iki:
- Mbusak tag sing ora aman (contone `script`, `iframe`, lsp.)
- Busak `on*` atribut acara
- Mbela serangan XSS kanthi nyaring atribut sing diwiwiti nganggo protokol mbebayani kayata `javascript:`, `vbscript:`, lan `data:` (kajaba `data:image/`).