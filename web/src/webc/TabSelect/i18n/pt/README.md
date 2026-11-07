# Página da guia com seleção suspensa

- Suporta a incorporação de caixa de seleção suspensa de largura adaptável (`<c-select>`) no rótulo, que só será exibida quando a página da guia atual for ativada.
- Especificação estrutural: o elemento filho usa `nav > a` como navegação de rótulo, e o elemento filho direto `b` serve como área de conteúdo.
- Associa automaticamente a tag `a[value]`, `a` com `c-select` (com base no atributo de valor do grupo e no valor da opção selecionada) e no nó de conteúdo `b[slot]`.
- Suporta a persistência automática do item atualmente ativado para `localStorage` por meio do atributo `key`.
- Suporta salvamento e restauração automáticos de subseleções históricas para cada grupo em `localStorage` usando `key + '/' + tab`.
- Suporta a especificação da guia ativa por meio do atributo `value`.

## Use a demonstração

```html
<c-tab-select key="my-tab-select-key" value="language/js">
  <nav>
    <a value="language">
      <span>linguagem</span>
      <c-select>
        <option value="js">JavaScript</option>
        <option value="rs">Rust</option>
      </c-select>
    </a>
    <a value="os">
      <span>sistema</span>
      <c-select>
        <option value="mac">macOS</option>
        <option value="win">Windows</option>
      </c-select>
    </a>
  </nav>
  <b slot="language">
    <b slot="js">JavaScript é uma linguagem de script</b>
    <b slot="rs">Rust é uma linguagem de programação em nível de sistema</b>
  </b>
  <b slot="os">
    <b slot="mac">macOS é um sistema operacional gráfico baseado em Unix</b>
    <b slot="win">Windows é um sistema operacional desenvolvido pela Microsoft</b>
  </b>
</c-tab-select>
```

## propriedade

- `key`: nome da chave usada para persistência localStorage
- `value`: valor atualmente selecionado

## Eventos personalizados

- `change`: Acionado ao alternar entre guias, `e.value` é o valor `value` da opção atualmente ativada.