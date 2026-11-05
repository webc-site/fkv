# Menu di navigazione adattato alle diverse dimensioni dello schermo

- Desktop: i collegamenti vengono visualizzati in riquadri orizzontali
- Versione mobile: Comprimi nel pulsante del menu, fai clic per far scorrere fuori il cassetto della barra laterale
- Attivazione del percorso: confronta automaticamente il percorso corrente e il valore hash e aggiungi il nome della classe dello stato di attivazione al collegamento corrispondente `A`
- Chiudi automaticamente: quando il terminale mobile fa clic su un collegamento o sullo sfondo esterno della barra laterale, la barra laterale si chiude automaticamente.

## utilizzo

```html
<c-nav-menu>
  <a href="/">prima pagina</a>
  <a href="/doc">documento</a>
  <a href="/about">Di</a>
</c-nav-menu>
```

## illustrare

Quando il componente viene inizializzato, se non è presente `c-side` nell'elemento figlio, il pulsante del menu mobile e la barra laterale `c-side` verranno creati automaticamente e tutti i tag `a` verranno spostati nella barra laterale.

## lezione di stile

- **R**: Il collegamento attivato aggiungerà automaticamente il nome della classe `A`