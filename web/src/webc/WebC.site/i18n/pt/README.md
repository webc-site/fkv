# Portal de roteamento que hospeda documentação da biblioteca de componentes e site de demonstração

- **Layout em tela cheia**: o posicionamento absoluto preenche a janela de visualização, com rolagem e navegação integradas.
- **Roteamento dinâmico**: analise caminhos de URL e carregue dinamicamente demonstrações de componentes ou páginas iniciais.
- **Multilíngue**: responda e atualize a barra de navegação e o texto da página em tempo real.
- **Mudança de status**: mantém automaticamente as classes de estilo de carregamento e página inicial.

## Use a demonstração

```html
<c-webc-site></c-webc-site>
```

## aula de estilo

O componente mudará os seguintes nomes de classe dentro de si:

- `Ing`: carregamento de rota ou análise de pacote de idiomas.
- `A`: Atualmente na página inicial.