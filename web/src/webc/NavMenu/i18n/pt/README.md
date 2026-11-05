# Menu de navegação adaptado a diferentes tamanhos de tela

- Desktop: os links são exibidos em blocos horizontais
- Versão móvel: botão Recolher no menu, clique para deslizar a gaveta da barra lateral
- Ativação de rota: compare automaticamente o caminho atual e o valor de hash e adicione o nome da classe de status de ativação ao link correspondente `A`
- Fechar automaticamente: Quando o terminal móvel clica em um link ou no fundo externo da barra lateral, a barra lateral fecha automaticamente.

## usar

```html
<c-nav-menu>
  <a href="/">primeira página</a>
  <a href="/doc">documento</a>
  <a href="/about">sobre</a>
</c-nav-menu>
```

## ilustrar

Quando o componente é inicializado, se não houver `c-side` no elemento filho, o botão do menu móvel e a barra lateral `c-side` serão criados automaticamente e todas as tags `a` serão movidas para a barra lateral.

## aula de estilo

- **A**: O link ativado adicionará automaticamente o nome da classe `A`