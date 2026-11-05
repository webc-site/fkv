# Componente de postagem de blog

- **Agrupamento automático de anos**: Classifique automaticamente os anos de acordo com a data do artigo e organize-os na ordem inversa por ano
- **Formato de data simplificado**: remova automaticamente o prefixo do ano ao exibir e exiba apenas o mês e o dia
- **Resumo e texto de markdown**: visualização integrada do resumo da lista e renderização de texto completo de artigo único
- **Título tridimensional gradiente**: integre `H1Txt` para exibir o título e a data de lançamento

## Use a demonstração

```html
<c-blog></c-blog>

<script type="module">
import "webc.site/Blog.js";

const blog = document.querySelector("c-blog");
blog.prefix = "/blog/";
blog.mdUrl = (key, lang) => "/doc/" + lang + "/blog/" + key + ".md";
blog._ = {
  "2026/publicar": ["título", "resumo", "2026-07-27"]
};

// Alterna para carregar o corpo do artigo especificado (passa uma string vazia para retornar à lista)
blog.load("2026/publish");
</script>
```

## Parâmetros de interface

### Propriedades e métodos

- `blog.prefix`: Obtenha ou defina o prefixo do link do artigo (como `"/blog/"`)
- `blog.mdUrl`: Obtenha ou defina a função de geração de endereço de artigo Markdown `(key, lang) => url`
- `blog._`: Obtenha ou defina o dicionário de metadados da lista de artigos `{ [key]: [title, summary, date] }`
- `blog.load(key)`: alterne para carregar o conteúdo do artigo da chave especificada, passe uma string vazia `""` para retornar à lista de blogs