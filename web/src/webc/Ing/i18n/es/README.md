# Indicador de estado de carga o procesamiento.

- Unifique el cursor en estado de espera
- Muestra una animación ondulada para indicar el estado de procesamiento.
- Aplicar una máscara de desenfoque a un formulario para evitar envíos duplicados

## Usa la demostración

```html
<!-- indicador de carga independiente -->
<div class="Ing"></div>

<!-- Cargando formulario con mascarilla -->
<form class="Ing">
  <input type="text" placeholder="Cuadro de entrada">
  <button type="submit">entregar</button>
</form>
```

## clase de estilo

### `.Ing`
- El estado del puntero es `wait`.
- El pseudoelemento `::before` representa una animación de onda.
- Cuando se usa en el elemento `form`, el pseudoelemento `::after` representa una máscara de desenfoque de 1 px, lo que desactiva la interacción y la selección de texto.