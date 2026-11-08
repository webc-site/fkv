# prompt de bolha flutuante

Exibe informações de dica de ferramenta de foco quando o mouse passa ou o elemento é ativado.

- Baseado em Web Component nativo, leve e de alto desempenho
- Suporta posicionamento espacial adaptativo em quatro direções (class="superior/inferior/esquerda/direita")
- Ele usa um fundo gradiente de textura de vidro fosco preto e caracteres brancos, com destaques de contorno de borda física requintados.
- Cálculo adaptativo de posição integrado, ajusta automaticamente a direção do pop-up de acordo com o espaço restante na janela de visualização
- Use `slot="pop"` para exibir conteúdo de prompt flutuante

## Use a demonstração

```html
<!-- Calcular direção automaticamente (recomendado, sem necessidade de declarar classe) -->
<c-pophover>
  <button>automático</button>
  <div slot="pop">Esta é uma ponta de bolha</div>
</c-pophover>

<!-- direção fixa -->
<c-pophover class="left">
  <button>Esquerda</button>
  <div slot="pop">Esta é uma ponta de bolha</div>
</c-pophover>

<c-pophover class="right">
  <button>certo</button>
  <div slot="pop">Esta é uma ponta de bolha</div>
</c-pophover>

<c-pophover class="bottom">
  <button>Abaixo</button>
  <div slot="pop">Esta é uma ponta de bolha</div>
</c-pophover>

<c-pophover class="top">
  <button>superior</button>
  <div slot="pop">Esta é uma ponta de bolha</div>
</c-pophover>
```

## Variáveis ​​CSS

Os estilos personalizados são suportados pelas seguintes variáveis ​​CSS:

| nome da variável | valor padrão | ilustrar |
| :--- | :--- | :--- |
| `--pophover-gap` | `8px` | O espaço entre a dica de ferramenta da bolha e o elemento acionador |
| `--pophover-margin` | `24px` | Margem de segurança da borda da viewport durante o cálculo de posicionamento adaptativo |
| `--pophover-bg-top` | `#18181ce0` | Cor superior do fundo gradiente de bolha |
| `--pophover-bg-mid` | `#121215e5` | A cor de fundo das setas nos lados esquerdo e direito da bolha |
| `--pophover-bg-bottom` | `#0c0c0feb` | A cor das setas laterais inferior e superior e inferior do fundo gradiente de bolha |

## Estilo de dica de ferramenta do ícone (`.Ico`)

Para botões de prompt flutuantes do tipo ícone puro (como "Editar", "Excluir" etc.), você pode aplicar um ícone unificado e um estilo de foco adicionando o nome da classe `.Ico` ao contêiner externo ou ao próprio componente.

### Exemplo de estrutura

```html
<b class="Edit Ico">
  <c-pophover>
    <a></a>
    <div slot="pop">editar</div>
  </c-pophover>
</b>
```

As alterações básicas de layout, tamanho (18x18px) e cor de foco foram unificadas em `.Ico > c-pophover > a`. Os desenvolvedores só precisam especificar o arquivo de ícone correspondente separadamente, como:
```css
.Edit > c-pophover > a {
  mask-image: url("./svg/edit.svg")
}
```