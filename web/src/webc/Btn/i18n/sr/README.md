# Обично дугме и дугме округле иконе

## Назив класе

- Нормално дугме<б ид="1">
- дугме са иконом <б ид="1">
- Примарно истакнуто стање<б ид="1">

## користити

Додајте назив класе елементу:

```html
<!-- Нормално дугме -->
<button class="Btn">потврдити</button>

<!-- главни нагласак -->
<button class="Btn Main">поднети</button>

<!-- дугме за везу -->
<a class="Btn" href="https://github.com">дугме за везу</a>
```

## стил иконе

Икона за <б ид="2"> може да се прилагоди преко <б ид="1">:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```