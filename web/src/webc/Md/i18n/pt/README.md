# Analise e renderize texto Markdown em HTML com segurança.

- **Higienização de segurança**: defenda-se contra ataques XSS usando API `setHTML` nativa ou produtos de limpeza `DOMParser`
- **Extração de conteúdo**: lê automaticamente o conteúdo do texto do slot padrão durante a inicialização e o renderiza
- **Composição tipográfica integrada**: lista predefinida, tabela, bloco de código, citação e estilos de caixa de aviso

## Use a demonstração

### renderização estática

```html
<c-md>
# título
Este é um conteúdo do Markdown
</c-md>
```

### atribuição dinâmica

```html
<c-md id="md"></c-md>

<script type="module">
import "webc.site/Md.js";
document.querySelector("#md").value = "#Novo título\nNovo conteúdo";
</script>
```

## Parâmetros de interface

Passe o texto do Markdown diretamente na tag (como espaço padrão) ou atualize dinamicamente por meio de `value`.

## Descrição do estilo

A classe de estilo `.Md` é adicionada automaticamente quando o componente é inicializado. Suporta o seguinte layout de caixa de aviso estendido Markdown (acionado pela sintaxe de referência):

- `[!NOTE]`：Informações imediatas
- `[!TIP]`: Sugestões/Dicas
- `[!IMPORTANT]`: Atenção importante
- `[!WARNING]`: Aviso de risco
- `[!CAUTION]`: aviso de risco extremamente alto

## mecanismo de segurança

Ao renderizar, chame primeiro a API nativa `setHTML` para injetar HTML com segurança. Se o navegador não for compatível, ele fará o downgrade automaticamente para um limpador recursivo baseado em `DOMParser` e implementará a seguinte política de segurança:
- Remova tags não seguras (por exemplo, `script`, `iframe`, etc.)
- Limpar atributos de evento `on*`
- Defenda-se contra ataques XSS filtrando atributos começando com protocolos perigosos como `javascript:`, `vbscript:` e `data:` (exceto `data:image/`).