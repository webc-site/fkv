# Pestanyes canviables

- L'estat seleccionat de l'etiqueta s'integra perfectament amb l'àrea de contingut següent.
- Especificacions estructurals: els elements secundaris es fixen per utilitzar `nav > a` com a navegació d'etiquetes i l'element secundari directe `b` s'utilitza com a àrea de contingut.
- Associa automàticament l'etiqueta `a[value]` amb el node de contingut `b[slot]`.
- Admet la permanència automàtica de la pàgina activa actual a `localStorage` mitjançant l'atribut `key`.
- Admet l'especificació de la pàgina de pestanya activada mitjançant l'atribut `value` (si no hi ha memòria cau i `value`, l'etiqueta amb `class="A"` o la primera etiqueta es seleccionarà de manera predeterminada).
- Implementació lleugera de DOM lleuger, sense obstrucció Shadow DOM, alt grau de llibertat en la personalització de l'estil.

## Utilitzeu la demostració

```html
<c-tab key="my-tab-key" value="banana">
  <nav>
    <a value="apple">poma</a>
    <a value="banana">plàtan</a>
    <a value="orange">mandarina</a>
  </nav>
  <b slot="apple">La poma és una fruita nutritiva</b>
  <b slot="banana">El plàtan és una fruita tropical</b>
  <b slot="orange">Les taronges són riques en vitamina C</b>
</c-tab>
```

## propietat

- `key`: nom de clau utilitzat per a la persistència de localStorage
- `value`: valor seleccionat actualment

## Esdeveniments personalitzats

- `change`: activat quan es canvia la pestanya, `e.value` és el valor `value` de l'etiqueta activada actualment.