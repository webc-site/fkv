# Selezione a discesa, la larghezza si adatta all'elemento corrente

Per impostazione predefinita, la larghezza del `select` nativo è determinata dal testo dell'opzione più lungo. Quando si cambiano le opzioni brevi, c'è troppo spazio bianco, che influisce sull'aspetto.

Questo componente copre in modo trasparente il `select` nativo sopra il tag `b`, consentendo alla larghezza di adattarsi con precisione alla lunghezza del testo attualmente selezionato.

- Mantieni gli stili nativi del menu a discesa `select` e il supporto dell'accessibilità su tutti i terminali
- Supporta operazioni interattive native come la navigazione completa delle schede della tastiera e il cambio dei tasti di direzione.
- Adatta la larghezza all'opzione corrente per eliminare lo spazio bianco in eccesso

## Utilizza la demo

```html
<c-select>
  <option value="js">JavaScript</option>
  <option value="rs">Rust</option>
  <option value="py">Python</option>
</c-select>
```