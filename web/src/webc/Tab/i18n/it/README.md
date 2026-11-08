# Schede commutabili

- Lo stato selezionato dell'etichetta è perfettamente integrato con l'area del contenuto sottostante.
- Specifiche strutturali: gli elementi secondari sono fissati per utilizzare `nav > a` come navigazione dell'etichetta e l'elemento secondario diretto `b` viene utilizzato come area del contenuto.
- Associa automaticamente il tag `a[value]` al nodo del contenuto `b[slot]`.
- Supporta la persistenza automatica della pagina attiva corrente su `localStorage` tramite l'attributo `key`.
- Supporta la specifica della scheda attivata tramite l'attributo `value` (se non è presente cache e `value`, il tag con `class="A"` o il primo tag verrà selezionato per impostazione predefinita).
- Implementazione leggera del DOM leggero, nessuna ostruzione del DOM ombra, alto grado di libertà nella personalizzazione dello stile.

## Utilizza la demo

```html
<c-tab key="my-tab-key" value="banana">
  <nav>
    <a value="apple">mela</a>
    <a value="banana">banana</a>
    <a value="orange">mandarino</a>
  </nav>
  <b slot="apple">La mela è un frutto nutriente</b>
  <b slot="banana">La banana è un frutto tropicale</b>
  <b slot="orange">Le arance sono ricche di vitamina C</b>
</c-tab>
```

## proprietà

- `key`: nome della chiave utilizzato per la persistenza localStorage
- `value`: valore attualmente selezionato

## Eventi personalizzati

- `change`: attivato quando si cambia scheda, `e.value` è il valore `value` del tag attualmente attivato.