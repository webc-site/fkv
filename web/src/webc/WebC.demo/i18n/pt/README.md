# Renderize e exiba demonstrações de componentes, documentação e instruções de instalação

- Extraia e analise o título, introdução e conteúdo do documento README
- Sandbox para executar demonstrações interativas de componentes e isolar estilos
- Converta blocos de código em documentos em apresentações interativas
- Layout adaptável, exibição de colunas em tela ampla, exibição de alternância de guias em tela estreita

## Use a demonstração

```html
<c-webc-demo></c-webc-demo>
```

```js
import WebCDemo from "./_.js";

const el = document.querySelector("c-webc-demo");

//Inicializa e obtém a interface de operação
const [setMod, setMd, unMount] = WebCDemo(el);
```

## Parâmetros de interface

### `WebCDemo(el)`

Inicialize o componente.

- **parâmetro**
  - `el`:`HTMLElement`, contêiner de componente.
- **Valor de retorno**:`[setMod, setMd, unMount]`
  - `setMod(mod)`: Configure o módulo de demonstração.
  - `setMd(name, md, lang)`: Defina o nome do componente, o conteúdo do documento README e o idioma atual.
  - `unMount()`: Descarregue o componente e limpe o ouvinte de eventos.

### Atributo `mod` do módulo de demonstração

- `CSS`:`string`, o caminho da folha de estilo do componente.
- `HTM`:`string`, o modelo HTML do componente.
- `default`:`function(root)`, função de inicialização do componente, recebe contêiner raiz sombra.

## aula de estilo

### `.M`

área de conteúdo.

### `.L`

O painel esquerdo contém instruções e documentação.

### `.R`

Painel direito, contendo a demonstração.

### `c-nav`

Barra de navegação de tela estreita.