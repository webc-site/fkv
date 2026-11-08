# Depurando código HTML e JS online

- Renderize uma barra de depuração abaixo do código e forneça portais de depuração CodePen e JSFiddle
- Lembre-se automaticamente da plataforma de depuração selecionada
- Suporta injeção de retornos de chamada por meio do atributo `.gen` para gerar dinamicamente código HTML, CSS e JS
- Identifique automaticamente elementos filhos com o nome da classe `language-js` e execute no modo JS

## Use a demonstração

```html
<c-htm-demo><pre><code class="language-html">
<h1>Demonstração</h1>
</code></pre></c-htm-demo>

<script type="module">
import "webc.site/HtmDemo.js";

const el = document.querySelector("c-htm-demo");
el.gen = (type, text) => {
  //Retorna [html, css, js]
  return [
    text,
    "h1 { color: red; }",
    "console.log(1)"
  ];
};
</script>
```

## Parâmetros de interface

### Propriedades e métodos

- `.gen`: Função de retorno de chamada injetada. **Este retorno de chamada deve ser injetado para que a barra de depuração seja exibida. **
  - parâmetro:
    - `type`: Tipo de conteúdo injetado (`HTML = 1`, `JS = 2`).
    - `text`: o texto do código extraído do elemento filho.
  - Valor de retorno: `[htm, css, js]`