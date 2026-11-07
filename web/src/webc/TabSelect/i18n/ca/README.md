# Pestanya amb selecció desplegable

- Admet la inserció del quadre de selecció desplegable d'amplada adaptativa (`<c-select>`) a l'etiqueta, que només es mostrarà quan la pàgina de pestanya actual estigui activada.
- Especificació estructural: l'element secundari utilitza `nav > a` com a navegació d'etiquetes i l'element secundari directe `b` serveix com a àrea de contingut.
- Associa automàticament l'etiqueta `a[value]`, `a` amb `c-select` (segons l'atribut de valor del grup i el valor de l'opció seleccionada) i el node de contingut `b[slot]`.
- Admet la permanència automàtica de l'element activat actualment a `localStorage` mitjançant l'atribut `key`.
- Admet l'emmagatzematge i la restauració automàtics de subseleccions històriques per a cada grup a `localStorage` mitjançant `key + '/' + tab`.
- Admet l'especificació de la pestanya activa mitjançant l'atribut `value`.

## Utilitzeu la demostració

```html
<c-tab-select key="my-tab-select-key" value="language/js">
  <nav>
    <a value="language">
      <span>llengua</span>
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
    <b slot="js">JavaScript és un llenguatge de script</b>
    <b slot="rs">Rust és un llenguatge de programació a nivell de sistemes</b>
  </b>
  <b slot="os">
    <b slot="mac">macOS és un sistema operatiu gràfic basat en Unix</b>
    <b slot="win">Windows és un sistema operatiu desenvolupat per Microsoft</b>
  </b>
</c-tab-select>
```

## propietat

- `key`: nom de clau utilitzat per a la persistència de localStorage
- `value`: valor seleccionat actualment

## Esdeveniments personalitzats

- `change`: activat en canviar de pestanya, `e.value` és el valor `value` de l'opció activada actualment.