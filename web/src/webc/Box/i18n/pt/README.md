# Criar e exibir camada pop-up

- Chamada funcional para criar e exibir camada pop-up
- Remover automaticamente elementos do DOM quando fechado
- Impedir o comportamento de cancelamento da tecla Esc

## Use a demonstração

```html
<button>Clique para aparecer</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "Título";
  description.textContent = "Texto";
  button.className = "Btn Main";
  button.textContent = "OK";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## Parâmetros de interface

### Funções exportadas padrão

`Box()`

- **Valor de retorno**: `HTMLDialogElement`, o elemento dialog da classe de estilo `Box` foi adicionado.

## aula de estilo

### `.Box`

Aplica-se ao elemento `dialog` para definir o posicionamento, centralização e desfoque da máscara de fundo.

### `.Lg`

Uma classe de estilo aplicada ao conteúdo interno, definindo a cor de fundo, cantos arredondados, sombra e layout vertical, e definindo o layout do `h3` e `p` interno.