# Guias alternáveis

- O estado selecionado do rótulo é perfeitamente integrado à área de conteúdo abaixo.
- Especificações estruturais: os elementos filhos são fixados para usar `nav > a` como navegação de rótulo, e o elemento filho direto `b` é usado como área de conteúdo.
- Associe automaticamente a tag `a[value]` ao nó de conteúdo `b[slot]`.
- Suporta a persistência automática da página ativa atual para `localStorage` por meio do atributo `key`.
- Suporta a especificação da guia ativada por meio do atributo `value` (se não houver cache e `value`, a tag com `class="A"` ou a primeira tag será selecionada por padrão).
- Implementação leve do Light DOM, sem obstrução do Shadow DOM, alto grau de liberdade na personalização do estilo.

## Use a demonstração

```html
<c-tab key="my-tab-key" value="banana">
  <nav>
    <a value="apple">maçã</a>
    <a value="banana">banana</a>
    <a value="orange">tangerina</a>
  </nav>
  <b slot="apple">A maçã é uma fruta nutritiva</b>
  <b slot="banana">Banana é uma fruta tropical</b>
  <b slot="orange">Laranjas são ricas em vitamina C</b>
</c-tab>
```

## propriedade

- `key`: nome da chave usada para persistência localStorage
- `value`: valor atualmente selecionado

## Eventos personalizados

- `change`: Acionado quando a guia é trocada, `e.value` é o valor `value` da tag atualmente ativada.