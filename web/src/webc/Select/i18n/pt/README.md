# Seleção suspensa, a largura se adapta ao item atual

A largura do `select` nativo é determinada pelo texto de opção mais longo por padrão. Ao alternar as opções curtas, há muito espaço em branco, o que afeta a aparência.

Este componente cobre de forma transparente o `select` nativo acima da tag `b`, permitindo que a largura se adapte com precisão ao comprimento do texto atualmente selecionado.

- Manter estilos de menu suspenso nativos `select` e suporte de acessibilidade em todos os terminais
- Suporta operações interativas nativas, como navegação completa por guias no teclado e alternância de teclas de direção.
- Adapte a largura à opção atual para eliminar o excesso de espaço em branco

## Use a demonstração

```html
<c-select>
  <option value="js">JavaScript</option>
  <option value="rs">Rust</option>
  <option value="py">Python</option>
</c-select>
```