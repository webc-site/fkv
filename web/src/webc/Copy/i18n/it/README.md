# Copia il testo negli appunti

Fare clic sul pulsante per copiare il testo dell'elemento specificato.

- Supporta l'utilizzo di rel per specificare il selettore di elementi da copiare
- Se non specificato, l'elemento fratello precedente viene copiato per impostazione predefinita.
- Finestra di messaggio Pophover nidificata
- Supporto internazionale

## Utilizza la demo

```html
<!-- Copia elementi specifici -->
<span id="target">Ecco il testo da copiare</span>
<c-copy rel="#target"></c-copy>

<!-- Gli elementi precedenti vengono copiati per impostazione predefinita -->
<span>il testo dell'elemento precedente</span>
<c-copy></c-copy>
```

## parametro

| proprietà | illustrare |
| --- | --- |
| rel | Copia il selettore dell'elemento di destinazione. Opzionale |

## stile di stato

- `c-pophover.Done`: stile della casella di messaggio dopo la copia riuscita