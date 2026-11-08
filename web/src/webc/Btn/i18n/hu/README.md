# Közönséges gomb és kerek ikon gomb

## Osztály neve

- Normál gomb`.Btn`
- ikon gomb `.BtnC`
- Elsődleges kiemelési állapot`.Btn.Main`

## használat

Adjon hozzá egy osztálynevet az elemhez:

```html
<!-- Normál gomb -->
<button class="Btn">erősítse meg</button>

<!-- fő fénypontja -->
<button class="Btn Main">benyújtani</button>

<!-- link gomb -->
<a class="Btn" href="https://github.com">link gomb</a>
```

## ikon stílus

A `BtnC` ikonja testreszabható a `background-image` segítségével:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```