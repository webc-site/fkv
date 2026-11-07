# Pannelli dei cassetti estraibili lateralmente

- Crea automaticamente strutture di scorrimento interne
- Fare clic sull'area esterna per comprimerla automaticamente
- Scorri verso sinistra e verso destra per chiudere
- Posizionamento scorrevole laterale nelle direzioni sinistra e destra
- Fornire eventi di apertura e chiusura

## Utilizza la demo

```html
<button class="open-btn">Aprire</button>

<c-side class="right">
  <div>Contenuto del cassetto</div>
</c-side>

<script type="module">
import { open } from "webc.site/Side.js";

const btn = document.querySelector(".open-btn"),
  side = document.querySelector("c-side");

btn.onclick = () => open(side);
</script>
```

## interfaccia

### metodo

- `open(el)`: funzione di esportazione, cassetto aperto
- `close(el)`: funzione di esportazione, chiusura del cassetto
- `el.open()`: metodo di istanza, cassetto aperto
- `el.close()`: metodo di istanza, chiude il drawer

### evento

- `open`: si attiva quando è acceso
- `close`: si attiva quando è chiuso

## lezione di stile

- `.right`: posizionamento dello slide out a destra
- `.top`: Posizionamento estraibile
- `.bottom`: scivola fuori dal posizionamento
- (Se non impostato, l'impostazione predefinita è scorrere verso sinistra)
- `.open`: stato aperto
- `.H`: stato nascosto
- `.T`: stato di transizione dell'animazione