# Botwm cyffredin a botwm eicon crwn

## Enw dosbarth

- Botwm arferol`.Btn`
- botwm eicon `.BtnC`
- Cyflwr uchafbwynt cynradd`.Btn.Main`

## defnydd

Ychwanegu enw dosbarth at yr elfen:

```html
<!-- Botwm arferol -->
<button class="Btn">cadarnhau</button>

<!-- prif uchafbwynt -->
<button class="Btn Main">cyflwyno</button>

<!-- botwm cyswllt -->
<a class="Btn" href="https://github.com">botwm cyswllt</a>
```

## arddull eicon

Gellir addasu'r eicon ar gyfer `BtnC` trwy `background-image`:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```