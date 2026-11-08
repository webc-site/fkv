# Indicador de estado de carga ou procesamento

- Unifica o cursor no estado de espera
- Mostra animación ondulada para indicar o estado de procesamento
- Aplique unha máscara de desenfoque a un formulario para evitar envíos duplicados

## Use a demostración

```html
<!-- indicador de carga independente -->
<div class="Ing"></div>

<!-- Cargando formulario con máscara -->
<form class="Ing">
  <input type="text" placeholder="Caixa de entrada">
  <button type="submit">enviar</button>
</form>
```

## clase de estilo

### `.Ing`
- O estado do punteiro é `wait`.
- O pseudoelemento `::before` representa unha animación ondulada.
- Cando se usa no elemento `form`, o pseudoelemento `::after` mostra unha máscara de desenfoque de 1 px, desactivando a interacción e a selección de texto.