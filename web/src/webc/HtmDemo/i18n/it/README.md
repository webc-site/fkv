# Debug del codice HTML e JS online

- Visualizza una barra di debug sotto il codice e fornisci i portali di debug CodePen e JSFiddle
- Ricorda automaticamente la piattaforma di debug selezionata
- Supporta l'inserimento di callback tramite l'attributo `.gen` per generare dinamicamente codice HTML, CSS e JS
- Identifica automaticamente gli elementi secondari con il nome della classe `language-js` ed eseguili in modalità JS

## Utilizza la demo

```html
<c-htm-demo><pre><code class="language-html">
<h1>Dimostrazione</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  // Restituisce [html, css, js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## Parametri dell'interfaccia

### Proprietà e metodi

- `.gen`: funzione di callback inserita. **Questo callback deve essere inserito affinché la barra di debug venga visualizzata. **
  - parametro:
    - `type`: tipo di contenuto inserito (`HTML = 1`, `JS = 2`).
    - `text`: il testo del codice estratto dall'elemento figlio.
  - Valore restituito: `[htm, css, js]`