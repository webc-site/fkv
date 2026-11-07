# messaggio di bolla mobile

Visualizza le informazioni sulla descrizione comando al passaggio del mouse quando si passa il mouse o l'elemento è attivato.

- Basato su componente Web nativo, leggero e performante
- Supporta il posizionamento spaziale adattivo in quattro direzioni (class="top/bottom/left/right")
- Utilizza uno sfondo sfumato con texture in vetro smerigliato nero e caratteri bianchi, con squisiti punti salienti del contorno del bordo fisico.
- Calcolo adattivo della posizione integrato, regola automaticamente la direzione del pop-up in base allo spazio rimanente nella finestra
- Utilizzare `slot="pop"` per visualizzare il contenuto del prompt mobile

## Utilizza la demo

```html
<!-- Calcola automaticamente la direzione (consigliato, non è necessario dichiarare la classe) -->
<c-pophover>
  <button>automatico</button>
  <div slot="pop">Questo è un suggerimento a bolle</div>
</c-pophover>

<!-- direzione fissa -->
<c-pophover class="left">
  <button>Sinistra</button>
  <div slot="pop">Questo è un suggerimento a bolle</div>
</c-pophover>

<c-pophover class="right">
  <button>Giusto</button>
  <div slot="pop">Questo è un suggerimento a bolle</div>
</c-pophover>

<c-pophover class="bottom">
  <button>Giù</button>
  <div slot="pop">Questo è un suggerimento a bolle</div>
</c-pophover>

<c-pophover class="top">
  <button>superiore</button>
  <div slot="pop">Questo è un suggerimento a bolle</div>
</c-pophover>
```

## Variabili CSS

Gli stili personalizzati sono supportati tramite le seguenti variabili CSS:

| nome della variabile | valore predefinito | illustrare |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | Lo spazio tra la descrizione comando a bolle e l'elemento di attivazione |
| `--pophover-margin` | `24px` | Margine di sicurezza del bordo della finestra durante il calcolo del posizionamento adattivo |
| `--pophover-bg-top` | `#18181ce0` | Colore superiore dello sfondo sfumato a bolle |
| `--pophover-bg-mid` | `#121215e5` | Il colore di sfondo delle frecce sui lati sinistro e destro della bolla |
| `--pophover-bg-bottom` | `#0c0c0feb` | Il colore delle frecce laterali inferiore e superiore e inferiore dello sfondo sfumato a bolle |

## Stile descrizione comando icona (`.Ico`)

Per i pulsanti di prompt mobili di tipo icona pura (come "Modifica", "Elimina" ecc.), puoi applicare un'icona unificata e uno stile al passaggio del mouse aggiungendo il nome della classe `.Ico` al contenitore esterno o al componente stesso.

### Esempio di struttura

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">modificare</div>
  </c-pophover>
</b>
```

Il layout di base, le dimensioni (18x18px) e le modifiche al colore al passaggio del mouse sono stati unificati in `.Ico > c-pophover > a`. Gli sviluppatori devono solo specificare separatamente il file icona corrispondente, ad esempio:
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```