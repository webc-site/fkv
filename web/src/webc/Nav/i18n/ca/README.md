# Barra de navegació superior que es replega o s'expandeix automàticament quan es desplaça la pàgina

- **Oculta automàticament**: s'amaga automàticament quan es desplaça cap avall per la pàgina.
- **Expansió automàtica**: es mostra automàticament quan la pàgina es desplaça cap amunt o la barra de desplaçament desapareix.
- **Optimització del rendiment**: utilitzeu `requestAnimationFrame` per accelerar el desplaçament.
- **Recursos de llançament**: els oients i observadors d'esdeveniments s'eliminen automàticament quan es destrueix el component.

## Utilitzeu la demostració

```html
<main>
  <c-vs>
    <nav class="Nav">
      <c-nav>
        <a class="logo" href="#">LOGO</a>
      </c-nav>
      <c-nav>
        <a class="item" href="#">Enllaç</a>
      </c-nav>
    </nav>
    <div class="content">Llisca cap avall per amagar la navegació, llisca cap amunt per mostrar-la</div>
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

## Instruccions d'ús

S'ha d'utilitzar com a element secundari o descendent de `<c-vs>`.

## estat d'estil

El component canvia el nom de classe següent a `nav.Nav`:

- `D`: mostra l'estat (fes lliscar cap avall o cap amunt).
- `I`: s'està amagant l'estat de transició.
- `H`: estat ocult.

## variables d'estil

- `--top`: s'actualitza automàticament pel component a un valor negatiu d'alçada, utilitzat per al posicionament i la transició.