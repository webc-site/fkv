# Tavallinen painike ja pyöreä kuvakepainike

## Luokan nimi

- Normaali painike`.Btn`
- kuvakepainike `.BtnC`
- Ensisijainen korostustila`.Btn.Main`

## käyttää

Lisää luokan nimi elementtiin:

```html
<!-- Normaali painike -->
<button class="Btn">vahvistaa</button>

<!-- pääkohokohta -->
<button class="Btn Main">lähetä</button>

<!-- linkkipainike -->
<a class="Btn" href="https://github.com">linkkipainike</a>
```

## ikoni tyyli

Kohteen `BtnC` kuvaketta voidaan mukauttaa `background-image`:n kautta:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```