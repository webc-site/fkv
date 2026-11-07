# Analizza e visualizza in modo sicuro il testo Markdown in HTML.

- **Sanificazione della sicurezza**: difesa dagli attacchi XSS utilizzando `setHTML` API native o `DOMParser` pulitori
- **Estrazione contenuto**: legge automaticamente il contenuto testuale dello slot predefinito durante l'inizializzazione e ne esegue il rendering
- **Impostazione incorporata**: stili di elenchi preimpostati, tabelle, blocchi di codice, virgolette e caselle di avviso

## Utilizza la demo

### resa statica

```html
<c-md>
# titolo
Questo è un pezzo di contenuto Markdown
</c-md>
```

### assegnazione dinamica

```html
<c-md id="md"></c-md>

<script type="module">
import "webc.site/Md.js";
document.querySelector("#md").value = "#Nuovo titolo\nNuovo contenuto";
</script>
```

## Parametri dell'interfaccia

Inserisci il testo Markdown direttamente all'interno del tag (come slot predefinito) o aggiornalo dinamicamente tramite `value`.

## Descrizione dello stile

La classe di stile `.Md` viene aggiunta automaticamente quando il componente viene inizializzato. Supporta il seguente layout della casella di avviso esteso Markdown (attivato dalla sintassi di riferimento):

- `[!NOTE]`：Informazioni richieste
- `[!TIP]`: Suggerimenti/Suggerimenti
- `[!IMPORTANT]`: Attenzione importante
- `[!WARNING]`: Avviso di rischio
- `[!CAUTION]`: avviso di rischio estremamente elevato

## meccanismo di sicurezza

Durante il rendering, chiama prima l'API `setHTML` nativa per inserire HTML in modo sicuro. Se il browser non lo supporta, verrà automaticamente eseguito il downgrade a un pulitore ricorsivo basato su `DOMParser` e implementerà la seguente politica di sicurezza:
- Rimuovi i tag non sicuri (ad esempio `script`, `iframe`, ecc.)
- Cancella gli attributi dell'evento `on*`
- Difenditi dagli attacchi XSS filtrando gli attributi che iniziano con protocolli pericolosi come `javascript:`, `vbscript:` e `data:` (eccetto `data:image/`).