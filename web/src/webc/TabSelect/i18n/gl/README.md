# Páxina de pestanas con selección despregable

- Admite incrustar a caixa de selección despregable de ancho adaptativo (`<c-select>`) na etiqueta, que só se mostrará cando estea activada a páxina de pestanas actual.
- Especificación estrutural: o elemento fillo usa `nav > a` como navegación de etiquetas e o elemento fillo directo `b` serve como área de contido.
- Asocia automaticamente a etiqueta `a[value]`, `a` con `c-select` (en función do atributo de valor do grupo e do valor da opción seleccionada) e o nodo de contido `b[slot]`.
- Admite a persistencia automática do elemento activado actualmente en `localStorage` mediante o atributo `key`.
- Admite o gardado automático e a restauración de subseleccións históricas de cada grupo en `localStorage` mediante `key + '/' + tab`.
- Admite especificar a pestana activa mediante o atributo `value`.

## Use a demostración

```html
<c-tab-select key="my-tab-select-key" value="language/js">
  <nav>
    <a value="language">
      <span>lingua</span>
      <c-select>
        <option value="js">JavaScript</option>
        <option value="rs">Rust</option>
      </c-select>
    </a>
    <a value="os">
      <span>sistema</span>
      <c-select>
        <option value="mac">macOS</option>
        <option value="win">Windows</option>
      </c-select>
    </a>
  </nav>
  <b slot="language">
    <b slot="js">JavaScript é unha linguaxe de script</b>
    <b slot="rs">Rust é unha linguaxe de programación a nivel de sistemas</b>
  </b>
  <b slot="os">
    <b slot="mac">macOS é un sistema operativo gráfico baseado en Unix</b>
    <b slot="win">Windows é un sistema operativo desenvolvido por Microsoft</b>
  </b>
</c-tab-select>
```

## propiedade

- `key`: nome da chave usado para a persistencia de LocalStorage
- `value`: valor seleccionado actualmente

## Eventos personalizados

- `change`: activado ao cambiar de pestana, `e.value` é o valor `value` da opción activada actualmente.