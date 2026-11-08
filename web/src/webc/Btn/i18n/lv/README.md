# Parasta poga un apaļa ikonas poga

## Klases nosaukums

- Parasta poga`.Btn`
- ikonas poga `.BtnC`
- Primārā izcelšanas statuss`.Btn.Main`

## izmantot

Pievienojiet elementam klases nosaukumu:

```html
<!-- Parasta poga -->
<button class="Btn">apstipriniet</button>

<!-- galvenais akcents -->
<button class="Btn Main">iesniegt</button>

<!-- saites poga -->
<a class="Btn" href="https://github.com">saites poga</a>
```

## ikonu stils

`BtnC` ikonu var pielāgot, izmantojot `background-image`:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```