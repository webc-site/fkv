# File comutabile

- Starea selectată a etichetei este integrată perfect cu zona de conținut de mai jos.
- Specificații structurale: elementele secundare sunt fixate pentru a utiliza `nav > a` ca navigare pentru etichete, iar elementul secundar direct `b` este folosit ca zonă de conținut.
- Asociați automat eticheta `a[value]` cu nodul de conținut `b[slot]`.
- Acceptă menținerea automată a paginii active curente la `localStorage` prin atributul `key`.
- Acceptă specificarea paginii de filă activată prin atributul `value` (dacă nu există cache și `value`, eticheta cu `class="A"` sau prima etichetă va fi selectată implicit).
- Implementare ușoară Light DOM, fără obstrucție Shadow DOM, grad ridicat de libertate în personalizarea stilului.

## Utilizați demonstrația

```html
<c-tab key="my-tab-key" value="banana">
  <nav>
    <a value="apple">măr</a>
    <a value="banana">banană</a>
    <a value="orange">mandarină</a>
  </nav>
  <b slot="apple">Mărul este un fruct hrănitor</b>
  <b slot="banana">Banana este un fruct tropical</b>
  <b slot="orange">Portocalele sunt bogate în vitamina C</b>
</c-tab>
```

## proprietate

- `key`: numele cheii folosit pentru persistența localStorage
- `value`: valoarea selectată în prezent

## Evenimente personalizate

- `change`: declanșat atunci când fila este comutată, `e.value` este valoarea `value` a etichetei activate în prezent.