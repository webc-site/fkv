# Indikator stanja nalaganja ali obdelave

- Poenotite kazalec v stanje čakanja
- Prikazuje valovito animacijo, ki označuje stanje obdelave
- Na obrazec uporabite masko za zameglitev, da preprečite podvojene oddaje

## Uporabite demo

```html
<!-- neodvisen indikator obremenitve -->
<div class="Ing"></div>

<!-- Nakladalni obrazec z masko -->
<form class="Ing">
  <input type="text" placeholder="Vnosno polje">
  <button type="submit">predložiti</button>
</form>
```

## stilski razred

### `.Ing`
- Stanje kazalca je `wait`.
- Psevdo element `::before` upodablja valovito animacijo.
- Ko se uporabi na elementu `form`, psevdoelement `::after` upodobi masko zameglitve velikosti 1 slikovne pike, ki onemogoči interakcijo in izbiro besedila.