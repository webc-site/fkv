# Crie e exiba uma camada pop-up com um botão Fechar

- Chamada funcional para criar e exibir camada pop-up
- Botão Fechar integrado, clique para fechar e remover o elemento do DOM
- Pressione Esc para desfocar a caixa de entrada ou fechar a camada pop-up

## Use a demonstração

```html
<button>Clique para aparecer</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "texto";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## Parâmetros de interface

A função de exportação padrão retorna `HTMLDialogElement`, e as classes de estilo `Box` e `X` foram adicionadas ao elemento de diálogo.

## aula de estilo

### `.Box.X`

Aplicado ao elemento `dialog`, especifica o estilo da caixa de diálogo.

### `a.X`

Aplicado ao elemento `a`, especifica o estilo do botão fechar.