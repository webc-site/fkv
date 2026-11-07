# Обично копче и копче за тркалезна икона

## Име на класата

- Нормално копче`.Btn`
- копче за икона `.BtnC`
- Примарна состојба на означување`.Btn.Main`

## употреба

Додадете име на класа во елементот:

```html
<!-- Нормално копче -->
<button class="Btn">потврди</button>

<!-- главен белег -->
<button class="Btn Main">поднесете</button>

<!-- копче за врска -->
<a class="Btn" href="https://github.com">копче за врска</a>
```

## стил на икона

Иконата за `BtnC` може да се приспособи преку `background-image`:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```