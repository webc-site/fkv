# Обычная кнопка и кнопка с круглым значком

## Имя класса

- Обычная кнопка`.Btn`
- кнопка со значком `.BtnC`
- Основное состояние выделения`.Btn.Main`

## использовать

Добавьте имя класса к элементу:

```html
<!-- Обычная кнопка -->
<button class="Btn">подтверждать</button>

<!-- главная изюминка -->
<button class="Btn Main">представлять на рассмотрение</button>

<!-- кнопка ссылки -->
<a class="Btn" href="https://github.com">кнопка ссылки</a>
```

## стиль иконки

Значок для `BtnC` можно настроить с помощью `background-image`:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```