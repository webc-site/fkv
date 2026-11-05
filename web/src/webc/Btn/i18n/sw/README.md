# Kitufe cha kawaida na kitufe cha ikoni ya pande zote

## Jina la darasa

- Kitufe cha kawaida`.Btn`
- kitufe cha ikoni `.BtnC`
- Hali kuu ya kuangazia`.Btn.Main`

## kutumia

Ongeza jina la darasa kwenye kipengele:

```html
<!-- Kitufe cha kawaida -->
<button class="Btn">thibitisha</button>

<!-- kivutio kikuu -->
<button class="Btn Main">wasilisha</button>

<!-- kitufe cha kiungo -->
<a class="Btn" href="https://github.com">kitufe cha kiungo</a>
```

## mtindo wa ikoni

Aikoni ya `BtnC` inaweza kubinafsishwa kupitia `background-image`:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```