# Venjulegur hnappur og hringlaga táknhnappur

## Nafn bekkjar

- Venjulegur hnappur`.Btn`
- táknhnappur `.BtnC`
- Aðal hápunktur ástand`.Btn.Main`

## nota

Bættu flokksheiti við frumefnið:

```html
<!-- Venjulegur hnappur -->
<button class="Btn">staðfesta</button>

<!-- aðal hápunktur -->
<button class="Btn Main">leggja fram</button>

<!-- tengihnappur -->
<a class="Btn" href="https://github.com">tengihnappur</a>
```

## tákn stíl

Táknið fyrir `BtnC` er hægt að aðlaga með `background-image`:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```