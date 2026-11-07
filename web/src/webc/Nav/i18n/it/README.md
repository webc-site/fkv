# Barra di navigazione superiore che si comprime o si espande automaticamente quando la pagina scorre

- **Nascondi automaticamente**: nascondi automaticamente quando scorri la pagina verso il basso.
- **Espansione automatica**: visualizzato automaticamente quando la pagina viene fatta scorrere verso l'alto o la barra di scorrimento scompare.
- **Ottimizzazione delle prestazioni**: utilizza `requestAnimationFrame` per limitare lo scorrimento.
- **Risorse di rilascio**: i listener e gli osservatori di eventi vengono automaticamente rimossi quando il componente viene distrutto.

## Utilizza la demo

```html
<main>
  <c-vs>
    <nav class="Nav">
      <c-nav>
        <a class="logo" href="#">LOGO</a>
      </c-nav>
      <c-nav>
        <a class="item" href="#">Collegamento</a>
      </c-nav>
    </nav>
    <div class="content">Scorri verso il basso per nascondere la navigazione, scorri verso l'alto per mostrarla</div>
  </c-vs>
</main>
<style>
  main {
    position: relative;
    height: 300px;
    border: 1px solid #000;
  }
  nav.Nav > c-nav {
    background: #fffc;
    backdrop-filter: blur(4px);
    border-bottom: 1px solid #ccc;
  }
  .content {
    height: 200dvh;
    background: linear-gradient(to bottom, #fff, #ccc);
    padding: 80px 20px 20px;
    color: #666;
    text-align: center;
  }
</style>
```

## Istruzioni per l'uso

Deve essere utilizzato come elemento figlio o discendente di `<c-vs>`.

## stato di stile

Il componente cambia il seguente nome di classe su `nav.Nav`:

- `D`: mostra lo stato (scorri verso il basso o in alto).
- `I`: stato di transizione nascosto.
- `H`: stato nascosto.

## variabili di stile

- `--top`: aggiornato automaticamente dal componente ad un valore negativo di altezza, utilizzato per il posizionamento e la transizione.