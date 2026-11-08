# Stili tipografici ottimizzati per l'esperienza di lettura

- **Impostazione di base**: adatta a titoli, paragrafi, grassetto, corsivo, barrato, codice incorporato, immagini e linee di divisione.
- **Animazione collegamento**: visualizza l'animazione di sottolineatura al passaggio del mouse.
- **Elenco progetti**: gli elenchi non ordinati utilizzano punti elenco, gli elenchi ordinati utilizzano numeri di serie con sfondo arrotondato e supportano l'annidamento multilivello.
- **Elenco attività**: fornisce lo stile della casella di controllo, supporta lo stato di cose da fare e completato.
- **Suggerimenti di avviso**: supporta cinque tipi di caselle di avviso: Nota, Suggerimento, Importante, Avviso e Attenzione.
- **Tabelle e codice**: fornisce l'evidenziazione delle righe della tabella al passaggio del mouse e blocchi di codice con ombre e sfondi sfumati.

## Utilizza la demo

```html
<div class="Md">
  <h1>titolo</h1>
  <p>Testo del paragrafo e <a href="#">link</a></p>
  <hr>
  <h2>Titolo di secondo livello</h2>
  <ul><li><input type="checkbox" checked="" disabled=""> Elemento attività</li></ul>
  <blockquote class="q note">
    <b class="qt"><i class="qi"></i>`<p>Nota contenuto</p>
  </blockquote>
</div>
```

## lezione di stile

Gli stili vengono applicati agli elementi HTML nel contenitore `.Md`.

### Stile casella di avviso

Aggiungi il seguente nome di classe sull'elemento `blockquote` con il nome di classe `.q` per attivare/disattivare la casella di avviso:

- `note`: Note
- `tip`: Suggerimento
- `important`: Importante
- `warning`: avviso
- `caution`: Nota