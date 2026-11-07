# barra de rolagem virtual

- **Rolagem bidirecional**: fornece rolagem vertical (`c-vs`) e horizontal (`c-hs`).
- **Resposta de tamanho**: monitore alterações no conteúdo e no tamanho da área, atualize automaticamente o tamanho e a posição da barra de rolagem e oculte automaticamente quando não houver excesso.
- **Suporte interativo**: Suporta rolagem de roda, arrastamento de controle deslizante e posicionamento de clique na trilha.
- **Ocultar status**: mostre e amplie o controle deslizante ao passar o mouse ou rolar e oculte-o automaticamente quando estiver parado.

## rolagem vertical

```html
<div style="height:300px;border:1px solid #000">
  <c-vs>
    <b>1</b>
    <b>2</b>
    <b>3</b>
  </c-vs>
</div>
<style>b { display: flex; border: 2px solid #090; padding: 30px; margin: 30px; }</style>
```

## rolagem horizontal

```html
<div style="width:300px;border:1px solid #000">
  <c-hs>
    <b>1</b>
    <b>2</b>
    <b>3</b>
  </c-hs>
</div>
<style>b { display: flex; border: 2px solid #090; margin: 30px; padding: 30px; }</style>
```

## peças de estilo

Personalize o estilo dos elementos internos do Shadow DOM via `::part()`:

- `::part(scroll)`: Role a área de conteúdo.
- `::part(bar)`: trilha da barra de rolagem.
- `::part(si)`: controle deslizante da barra de rolagem.
- `::part(i)`: Camada de envolvimento interna.
- `::part(drag)`: A trilha da barra de rolagem no estado de arrastar.

## aula de estilo

- `.drag`: Aplicado ao elemento `body`, indicando que ele está no estado de arrastar e rolar.

## Propriedades personalizadas

- `--si-bg`: Cor de fundo do controle deslizante.
- `--si-anim`: Animação do controle deslizante.
- `--cursorScrollhSvg`: O cursor do mouse quando a barra de rolagem horizontal é pairada.
- `--cursorScrollvSvg`: O cursor do mouse quando a barra de rolagem vertical é passada.