# Alternar automaticamente o menu de navegação e o botão de retorno

- Envolva automaticamente os elementos filhos para gerar o menu de navegação e as estruturas dos botões de retorno
- Rastreie automaticamente o histórico de rotas e calcule o caminho de retorno anterior
- Exibir o menu de navegação quando a rota corresponder ao item do menu; caso contrário, exibir o botão Voltar

## Use a demonstração

```html
<c-nav-l>
  <a href="/">primeira página</a>
  <a href="/doc">documento</a>
</c-nav-l>
```

## aula de estilo

- `B`: ativa o estado do botão de retorno. Mostre o botão Voltar e oculte o menu de navegação. Comutado automaticamente dentro do componente com base no roteamento.