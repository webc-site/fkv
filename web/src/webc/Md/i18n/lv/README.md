# Droši parsējiet un renderējiet Markdown tekstu HTML formātā.

- **Drošības dezinfekcija**: aizsardzība pret XSS uzbrukumiem, izmantojot vietējos `setHTML` API vai `DOMParser` tīrīšanas līdzekļus
- **Satura iegūšana**: automātiski nolasa noklusējuma slota teksta saturu inicializācijas laikā un atveido to
- **Iebūvēts salikums**: iepriekš iestatīts saraksts, tabula, kodu bloks, citātu un brīdinājuma lodziņu stili

## Izmantojiet demonstrāciju

### statiskā atveide

```html
<c-md>
# virsraksts
Šis ir Markdown satura gabals
</c-md>
```

### dinamisks uzdevums

```html
<c-md id="md"></c-md>

<script type="module">
import "webc.site/Md.js";
document.querySelector("#md").value = "#Jauns nosaukums\nJauns saturs";
</script>
```

## Interfeisa parametri

Ievietojiet Markdown tekstu tieši tagā (kā noklusējuma vietu) vai dinamiski atjauniniet, izmantojot `value`.

## Stila apraksts

Stila klase `.Md` tiek automātiski pievienota, kad komponents tiek inicializēts. Atbalsta šādu Markdown paplašinātā brīdinājuma lodziņa izkārtojumu (aktivizēja atsauces sintakse):

- `[!NOTE]`: tūlītēja informācija
- `[!TIP]`: ieteikumi/padomi
- `[!IMPORTANT]`: svarīga uzmanība
- `[!WARNING]`: brīdinājums par risku
- `[!CAUTION]`: īpaši augsta riska brīdinājums

## drošības mehānisms

Renderēšanas laikā vispirms izsauciet vietējo `setHTML` API, lai droši ievadītu HTML. Ja pārlūkprogramma to neatbalsta, tā automātiski pazeminās uz rekursīvo tīrītāju, pamatojoties uz `DOMParser`, un ieviesīs šādu drošības politiku:
- Noņemiet nedrošos tagus (piem., `script`, `iframe` utt.)
- Notīrīt `on*` notikumu atribūtus
- Aizsargājieties pret XSS uzbrukumiem, filtrējot atribūtus, sākot ar bīstamiem protokoliem, piemēram, `javascript:`, `vbscript:` un `data:` (izņemot `data:image/`).