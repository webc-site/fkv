# Karga edo prozesatzeko egoeraren adierazlea

- Bateratu kurtsorea itxaron egoeran
- Ripple animazioa erakusten du prozesatzeko egoera adierazteko
- Aplikatu lausotze-maskara bat inprimaki bati bidalketak bikoiztu ez daitezen

## Erabili demoa

```html
<!-- karga-adierazle independentea -->
<div class="Ing"></div>

<!-- Maskararekin inprimakia kargatzen -->
<form class="Ing">
  <input type="text" placeholder="Sarrera-koadroa">
  <button type="submit">aurkeztu</button>
</form>
```

## estilo klasea

### `.Ing`
- Erakuslearen egoera `wait` da.
- `::before` sasi-elementuak uhin-animazio bat errendatzen du.
- `form` elementuan erabiltzen denean, `::after` sasi-elementuak 1px lausotze-maskara errendatzen du, interakzioa eta testu-hautapena desgaituz.