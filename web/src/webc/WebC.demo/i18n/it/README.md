# Rendering e visualizzazione di demo, documentazione e istruzioni di installazione dei componenti

- Estrai e analizza il titolo, l'introduzione e il contenuto del documento README
- Sandbox per eseguire demo interattive di componenti e isolare stili
- Converti i blocchi di codice nei documenti in presentazioni interattive
- Layout adattivo, visualizzazione a colonne su schermo ampio, visualizzazione di commutazione delle schede su schermo stretto

## Utilizza la demo

```html
<c-webc-demo></c-webc-demo>
```

```js
import WebCDemo from "./_.js";

const el = document.querySelector("c-webc-demo");

//Inizializza e ottiene l'interfaccia operativa
const [setMod, setMd, unMount] = WebCDemo(el);
```

## Parametri dell'interfaccia

### `WebCDemo(el)`

Inizializzare il componente.

- **parametro**
  - `el`:`HTMLElement`, contenitore del componente.
- **Valore restituito**:`[setMod, setMd, unMount]`
  - `setMod(mod)`: configura il modulo demo.
  - `setMd(name, md, lang)`: imposta il nome del componente, il contenuto del documento README e la lingua corrente.
  - `unMount()`: scarica il componente e pulisci il listener di eventi.

### Attributo `mod` del modulo demo

- `CSS`:`string`, il percorso del foglio di stile del componente.
- `HTM`:`string`, il modello HTML del componente.
- `default`:`function(root)`, funzione di inizializzazione del componente, riceve il contenitore shadow root.

## lezione di stile

### `.M`

area dei contenuti.

### `.L`

Il pannello di sinistra contiene istruzioni e documentazione.

### `.R`

Pannello di destra, contenente la demo.

### `c-nav`

Barra di navigazione a schermo stretto.