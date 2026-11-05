# Fundo de vidro fosco e efeito de destaque de borda

A borda semitransparente e a sombra interna criam realces de borda em micro-relevo.
Como a dependência básica de estilo dos componentes do botão e da caixa de entrada.

## Use a demonstração

```html
<main class="Lg">
  <article>
    <h3>título</h3>
    <pre>Texto do conteúdo</pre>
  </article>
</main>
```

## aula de estilo

### `.Lg`
Estilo básico do cartão. Aplique `backdrop-filter` desfoque, `border` borda com micro-relevo e `box-shadow` sombra interna. Aplique o filtro `svg/glass.svg#h` ao passar o mouse.

### `main.Lg`
O contêiner do bloco principal da página. Cantos arredondados `24px` com preenchimento responsivo (`16px` se a largura for menor que `800px`, `24px` caso contrário).

### `.Btn`, `.BtnC`, `.Input`
Nome da classe do componente básico. Após introduzir este estilo, você obterá automaticamente um layout flexível, relações hierárquicas e propriedades básicas do vidro fosco.