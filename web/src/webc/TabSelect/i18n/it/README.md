# Scheda con selezione a discesa

- Supporta l'incorporamento della casella di selezione a discesa della larghezza adattiva (`<c-select>`) nell'etichetta, che verrà visualizzata solo quando viene attivata la scheda corrente.
- Specifica strutturale: l'elemento figlio utilizza `nav > a` come navigazione dell'etichetta e l'elemento figlio diretto `b` funge da area del contenuto.
- Associa automaticamente il tag `a[value]`, `a` a `c-select` (in base all'attributo value del gruppo e al valore dell'opzione selezionata) e al nodo del contenuto `b[slot]`.
- Supporta la persistenza automatica dell'elemento attualmente attivato su `localStorage` tramite l'attributo `key`.
- Supporta il salvataggio e il ripristino automatici delle sottoselezioni storiche per ciascun gruppo su `localStorage` utilizzando `key + '/' + tab`.
- Supporta la specifica della scheda attiva tramite l'attributo `value`.

## Utilizza la demo

```html
<c-tab-select key="my-tab-select-key" value="language/js">
  <nav>
    <a value="language">
      <span>lingua</span>
      <c-select>
        <option value="js">JavaScript</option>
        <option value="rs">Rust</option>
      </c-select>
    </a>
    <a value="os">
      <span>sistema</span>
      <c-select>
        <option value="mac">macOS</option>
        <option value="win">Windows</option>
      </c-select>
    </a>
  </nav>
  <b slot="language">
    <b slot="js">JavaScript è un linguaggio di scripting</b>
    <b slot="rs">Rust è un linguaggio di programmazione a livello di sistema</b>
  </b>
  <b slot="os">
    <b slot="mac">macOS è un sistema operativo grafico basato su Unix</b>
    <b slot="win">Windows è un sistema operativo sviluppato da Microsoft</b>
  </b>
</c-tab-select>
```

## proprietà

- `key`: nome della chiave utilizzato per la persistenza localStorage
- `value`: valore attualmente selezionato

## Eventi personalizzati

- `change`: attivato quando si passa da una scheda all'altra, `e.value` è il valore `value` dell'opzione attualmente attivata.