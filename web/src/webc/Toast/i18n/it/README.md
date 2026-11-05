# Finestra pop-up di richiesta sul bordo della pagina

- **Chiamata di funzione**: fornisce interfacce normali e di richiesta di errore
- **Impilazione automatica**: più puntali calcolano automaticamente la spaziatura e si impilano verticalmente
- **Arresto programmato e manuale**: supporta l'arresto programmato e manuale

## Utilizza la demo

```js
import { toast, toastErr } from "webc.site/js/Toast.js";

toast((el) => {
  el.textContent = "Contenuto della richiesta";
});

toastErr((el) => {
  el.textContent = "Contenuto errore";
});

toast(
  (el) => {
    el.textContent = "Chiudi automaticamente dopo 99 secondi";
  },
  99
);
```

## Parametri dell'interfaccia

### toast(render, timeout)

Creare e visualizzare prompt.

- `render`: `(el) => void`, rendering della richiamata, `el` è l'elemento della casella di messaggio.
- `timeout`: `Number`, secondi di timeout. Impostazione predefinita 9. Impostare su 0 per non spegnersi automaticamente.
- Valore restituito: elemento della casella di messaggio. Supporta `el.close()` disattivato.

### toastErr(render, timeout)

Creare e visualizzare messaggi di errore. I parametri sono gli stessi di `toast`, con la classe di stile `.ERR`.

## lezione di stile

- `.Toast`: classe base della casella di richiesta.
- `.ERR`: stato dell'errore.
- `.animated` / `.fadeInR` / `.fadeOutR`: transizione animata.
- `.x`: pulsante Chiudi.