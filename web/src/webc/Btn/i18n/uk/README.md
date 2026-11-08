# Звичайна кнопка та кнопка круглого значка

## Назва класу

- Звичайна кнопка`.Btn`
- значок кнопки `.BtnC`
- Основний стан виділення`.Btn.Main`

## використовувати

Додайте назву класу до елемента:

```html
<!-- Звичайна кнопка -->
<button class="Btn">підтвердити</button>

<!-- головна родзинка -->
<button class="Btn Main">подати</button>

<!-- кнопка посилання -->
<a class="Btn" href="https://github.com">кнопка посилання</a>
```

## стиль значка

Піктограму для `BtnC` можна налаштувати за допомогою `background-image`:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```