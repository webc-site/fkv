# Pestanas conmutables

- O estado seleccionado da etiqueta intégrase perfectamente coa área de contido que aparece a continuación.
- Especificacións estruturais: os elementos secundarios fixéronse para usar `nav > a` como navegación de etiquetas e o elemento fillo directo `b` úsase como área de contido.
- Asocia automaticamente a etiqueta `a[value]` co nodo de contido `b[slot]`.
- Admite a permanencia automática da páxina activa actual en `localStorage` mediante o atributo `key`.
- Admite especificar a páxina de pestanas activada a través do atributo `value` (se non hai caché e `value`, a etiqueta con `class="A"` ou a primeira etiqueta seleccionarase de forma predeterminada).
- Implementación lixeira do DOM lixeiro, sen obstrución do Shadow DOM, alto grao de liberdade na personalización do estilo.

## Use a demostración

```html
<c-tab key="my-tab-key" value="banana">
  <nav>
    <a value="apple">mazá</a>
    <a value="banana">plátano</a>
    <a value="orange">mandarina</a>
  </nav>
  <b slot="apple">A mazá é unha froita nutritiva</b>
  <b slot="banana">O plátano é unha froita tropical</b>
  <b slot="orange">As laranxas son ricas en vitamina C</b>
</c-tab>
```

## propiedade

- `key`: nome da chave usado para a persistencia de LocalStorage
- `value`: valor seleccionado actualmente

## Eventos personalizados

- `change`: activado cando se cambia a pestana, `e.value` é o valor `value` da etiqueta activada actualmente.