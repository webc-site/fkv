# Indicador d'estat de càrrega o processament

- Unifiqueu el cursor en estat d'espera
- Mostra una animació ondulada per indicar l'estat de processament
- Apliqueu una màscara de desenfocament a un formulari per evitar enviaments duplicats

## Utilitzeu la demostració

```html
<!-- indicador de càrrega independent -->
<div class="Ing"></div>

<!-- Càrrega del formulari amb màscara -->
<form class="Ing">
  <input type="text" placeholder="Caixa d'entrada">
  <button type="submit">presentar</button>
</form>
```

## classe d'estil

### `.Ing`
- L'estat del punter és `wait`.
- El pseudoelement `::before` representa una animació ondulada.
- Quan s'utilitza a l'element `form`, el pseudoelement `::after` mostra una màscara de desenfocament d'1 px, desactivant la interacció i la selecció de text.