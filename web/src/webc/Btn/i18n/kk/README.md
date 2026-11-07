# Кәдімгі түйме және дөңгелек белгіше түймесі

## Сынып атауы

- Қалыпты түйме`.Btn`
- белгіше түймесі `.BtnC`
- Негізгі бөлектеу күйі`.Btn.Main`

## пайдалану

Элементке сынып атауын қосыңыз:

```html
<!-- Қалыпты түйме -->
<button class="Btn">растау</button>

<!-- басты ерекшелігі -->
<button class="Btn Main">жіберу</button>

<!-- сілтеме түймесі -->
<a class="Btn" href="https://github.com">сілтеме түймесі</a>
```

## белгіше стилі

`BtnC` белгішесін `background-image` арқылы реттеуге болады:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```