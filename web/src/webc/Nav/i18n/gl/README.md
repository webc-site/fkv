# Barra de navegación superior que se contrae ou se expande automaticamente cando se despraza a páxina

- **Ocultar automaticamente**: ocultar automaticamente ao desprazarse pola páxina.
- **Expansión automática**: móstrase automaticamente cando a páxina se despraza cara arriba ou a barra de desprazamento desaparece.
- **Optimización do rendemento**: utiliza `requestAnimationFrame` para acelerar o desprazamento.
- **Recursos de lanzamento**: os oíntes e os observadores de eventos elimínanse automaticamente cando se destrúe o compoñente.

## Use a demostración

```html
<main>
  <c-vs>
    <nav class="Nav">
      <c-nav>
        <a class="logo" href="#">LOGO</a>
      </c-nav>
      <c-nav>
        <a class="item" href="#">Ligazón</a>
      </c-nav>
    </nav>
    <div class="content">Pasa o dedo cara abaixo para ocultar a navegación, pasa o dedo cara arriba para mostrala</div>
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

## Instrucións de uso

Debe usarse como elemento fillo ou descendente de `<c-vs>`.

## estado de estilo

O compoñente cambia o seguinte nome de clase en `nav.Nav`:

- `D`: Mostra o estado (desliza cara abaixo ou arriba).
- `I`: o estado de transición está oculto.
- `H`: estado oculto.

## variables de estilo

- `--top`: actualizado automaticamente polo compoñente a un valor negativo de altura, usado para o posicionamento e a transición.