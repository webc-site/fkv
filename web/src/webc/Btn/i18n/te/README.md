# సాధారణ బటన్ మరియు రౌండ్ ఐకాన్ బటన్

## తరగతి పేరు

- సాధారణ బటన్`.Btn`
- చిహ్నం బటన్ `.BtnC`
- ప్రాథమిక హైలైట్ స్థితి`.Btn.Main`

## ఉపయోగించండి

మూలకానికి తరగతి పేరును జోడించండి:

```html
<!-- సాధారణ బటన్ -->
<button class="Btn">నిర్ధారించండి</button>

<!-- ప్రధాన హైలైట్ -->
<button class="Btn Main">సమర్పించండి</button>

<!-- లింక్ బటన్ -->
<a class="Btn" href="https://github.com">లింక్ బటన్</a>
```

## చిహ్నం శైలి

`BtnC` కోసం చిహ్నాన్ని `background-image` ద్వారా అనుకూలీకరించవచ్చు:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```