# Componente post del blog

- **Raggruppamento automatico degli anni**: classifica automaticamente gli anni in base alla data dell'articolo e disponili in ordine inverso per anno
- **Formato data semplificato**: rimuove automaticamente il prefisso dell'anno durante la visualizzazione e visualizza solo il mese e il giorno
- **Riepilogo e testo del markdown**: anteprima del riepilogo dell'elenco integrato e rendering del testo completo del singolo articolo
- **Titolo tridimensionale sfumato**: integra `H1Txt` per visualizzare il titolo e la data di uscita

## Utilizza la demo

```html
<c-blog></c-blog>

<script type="module">
import "webc.site/Blog.js";

const blog = document.querySelector("c-blog");
blog.prefix = "/blog/";
blog.mdUrl = (key, lang) => "/doc/" + lang + "/blog/" + key + ".md";
blog._ = {
  "2026/pubblica": ["titolo", "abstract", "2026-07-27"]
};

// Passa per caricare il testo dell'articolo specificato (passa una stringa vuota per tornare all'elenco)
blog.load("2026/publish");
</script>
```

## Parametri dell'interfaccia

### Proprietà e metodi

- `blog.prefix`: ottieni o imposta il prefisso del collegamento all'articolo (come `"/blog/"`)
- `blog.mdUrl`: ottieni o imposta la funzione di generazione dell'indirizzo dell'articolo Markdown `(key, lang) => url`
- `blog._`: ottieni o imposta il dizionario dei metadati dell'elenco articoli `{ [key]: [title, summary, date] }`
- `blog.load(key)`: Passa per caricare il contenuto dell'articolo della chiave specificata, passa una stringa vuota `""` per tornare all'elenco dei blog