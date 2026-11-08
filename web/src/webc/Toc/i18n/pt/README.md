# Componente de navegação de diretório

- **Gaveta responsiva**: exibe a barra lateral em tela ampla, recolhe automaticamente em tela estreita e coopera com a gaveta `c-side` e o botão de acionamento de título
- **Driver multilíngue**: Suporta dicionário de objetos ou funções multilíngue, responde automaticamente à troca de idioma
- **Subdiretório automático**: analisa automaticamente os títulos `h1`~`h6` no contêiner de destino para gerar um diretório secundário e monitorar o destaque de rolagem da janela de visualização Hash.
- **Carregamento assíncrono**: clique em um item do diretório para obter automaticamente o texto Markdown de forma assíncrona e injetá-lo na área de conteúdo, exibindo a animação de carregamento `Ing`

## Use a demonstração

```html
<c-toc></c-toc>
<c-md></c-md>

<script type="module">
import "webc.site/Toc.js";

const toc = document.querySelector("c-toc");
toc.target = document.querySelector("c-md");
toc.prefix = "doc";
toc.mdUrl = (key, lang) => "/doc/" + lang + "/" + key + ".md";
toc.li = (lang) => ({
  "README": "WebC.site",
  "use": "usar componente"
});
</script>
```

## Parâmetros de interface

### Propriedades e métodos

- `toc.prefix`: Obtenha ou defina a string do prefixo de roteamento (como `"doc"`, a barra final é opcional), a chave `README` será automaticamente mapeada para um caminho vazio (ou seja, o próprio prefixo)
- `toc.li`: Obtenha ou defina a função de geração multilíngue do diretório `(lang) => ({ key: title })`
- `toc.mdUrl`: Obtenha ou defina a função de geração de URL Markdown `(key, lang) => url`
- `toc.target`: obtém ou define o nó de renderização de destino associado (como `c-md`). Se não for especificado, ele encontrará automaticamente `c-md` / `main` / `article`
- `toc.load(key)`: aciona manualmente o carregamento do documento e a expansão do diretório correspondente à chave
- `toc.sync()`: analise novamente manualmente o título no nó de destino atual, sincronize o diretório secundário e destaque o status