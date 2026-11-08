# Documentação e código-fonte para renderização de componentes ou funções de ferramentas

- Layout responsivo de duas colunas, com documentos exibidos à esquerda e código-fonte exibido à direita
- A documentação e o código-fonte suportam os botões de atalho Editar e Código para ir para o endereço de origem.
- O título e a introdução são exibidos na parte superior, suportando tradução automática i18n.

## Use a demonstração

```html
<c-webc-js src="/src/js/dom.js" href="/doc/{code}/js/dom.md"></c-webc-js>
```

## Parâmetros de propriedade

- `src`: endereço URL do código-fonte.
- `href`: endereço URL da marcação do documento, suporta o uso de espaço reservado `{code}` para adaptação ao idioma atual.