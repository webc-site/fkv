# Indicador de status de carregamento ou processamento

- Unifique o cursor no estado de espera
- Mostra animação ondulada para indicar o status do processamento
- Aplique uma máscara de desfoque a um formulário para evitar envios duplicados

## Use a demonstração

```html
<!-- indicador de carregamento independente -->
<div class="Ing"></div>

<!-- Carregando formulário com máscara -->
<form class="Ing">
  <bid="1">
  <button type="submit">enviar</button>
</form>
```

## aula de estilo

### `.Ing`
- O status do ponteiro é `wait`.
- O pseudoelemento `::before` renderiza uma animação ondulada.
- Quando usado no elemento `form`, o pseudoelemento `::after` renderiza uma máscara de desfoque de 1px, desativando a interação e a seleção de texto.