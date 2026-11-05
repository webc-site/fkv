# Janela pop-up de prompt na borda da página

- **Chamada de função**: Fornece interfaces normais e de prompt de erro
- **Empilhamento automático**: Várias pontas calculam automaticamente o espaçamento e empilham verticalmente
- **Desligamento programado e manual**: Suporta desligamento programado e manual

## Use a demonstração

```js
import { toast, toastErr } from "webc.site/js/Toast.js";

toast((el) => {
  el.textContent = "Conteúdo do prompt";
});

toastErr((el) => {
  el.textContent = "Conteúdo de erro";
});

toast(
  (el) => {
    el.textContent = "Fechar automaticamente após 99 segundos";
  },
  99
);
```

## Parâmetros de interface

### toast(render, timeout)

Crie e exiba prompts.

- `render`: `(el) => void`, retorno de chamada de renderização, `el` é o elemento da caixa de prompt.
- `timeout`: `Number`, tempo limite em segundos. Padrão 9. Defina como 0 para não desligar automaticamente.
- Valor de retorno: elemento da caixa de prompt. Suporta `el.close()` desativado.

### toastErr(render, timeout)

Crie e exiba mensagens de erro. Os parâmetros são os mesmos de `toast`, com classe de estilo `.ERR`.

## aula de estilo

- `.Toast`: Classe base da caixa de prompt.
- `.ERR`: Status do erro.
- `.animated` / `.fadeInR` / `.fadeOutR`: transição animada.
- `.x`: botão Fechar.