# Звычайная кнопка і кнопка з круглым значком

## Назва класа

- Звычайная кнопка`.Btn`
- значок кнопкі `.BtnC`
- Асноўны стан вылучэння`.Btn.Main`

## выкарыстоўваць

Дадайце назву класа да элемента:

```html
<!-- Звычайная кнопка -->
<button class="Btn">пацвердзіць</button>

<!-- галоўная разыначка -->
<button class="Btn Main">прадставіць</button>

<!-- кнопка спасылкі -->
<a class="Btn" href="https://github.com">кнопка спасылкі</a>
```

## стыль абразкоў

Значок для `BtnC` можна наладзіць праз `background-image`:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```