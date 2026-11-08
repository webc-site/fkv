# Barra de navegação superior que se recolhe ou expande automaticamente quando a página rola

- **Ocultar automaticamente**: ocultar automaticamente ao rolar a página para baixo.
- **Expandir automaticamente**: Exibido automaticamente quando a página é rolada para cima ou a barra de rolagem desaparece.
- **Otimização de desempenho**: use `requestAnimationFrame` para acelerar a rolagem.
- **Recursos de liberação**: ouvintes e observadores de eventos são removidos automaticamente quando o componente é destruído.

## Use a demonstração

```html
<main>
  <c-vs>
    <nav class="Nav">
      <c-nav>
        <a class="logo" href="#">LOGO</a>
      </c-nav>
      <c-nav>
        <a class="item" href="#">Link</a>
      </c-nav>
    </nav>
    <div class="content">Deslize para baixo para ocultar a navegação, deslize para cima para mostrá-la</div>
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

## Instruções de uso

Deve ser usado como elemento filho ou descendente de `<c-vs>`.

## status de estilo

O componente alterna o seguinte nome de classe em `nav.Nav`:

- `D`: Mostrar status (deslizar para baixo ou para cima).
- `I`: Estado de transição oculto.
- `H`: Status oculto.

## variáveis ​​de estilo

- `--top`: Atualizado automaticamente pelo componente para um valor negativo de altura, usado para posicionamento e transição.