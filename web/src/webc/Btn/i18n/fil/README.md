# Ordinaryong button at bilog na icon na button

## Pangalan ng klase

- Normal na button`.Btn`
- button ng icon `.BtnC`
- Pangunahing highlight state`.Btn.Main`

## gamitin

Magdagdag ng pangalan ng klase sa elemento:

```html
<!-- Normal na button -->
<button class="Btn">kumpirmahin</button>

<!-- pangunahing highlight -->
<button class="Btn Main">isumite</button>

<!-- pindutan ng link -->
<a class="Btn" href="https://github.com">pindutan ng link</a>
```

## estilo ng icon

Ang icon para sa `BtnC` ay maaaring i-customize sa pamamagitan ng `background-image`:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```