# சாதாரண பொத்தான் மற்றும் வட்ட ஐகான் பொத்தான்

## வகுப்பின் பெயர்

- இயல்பான பொத்தான்`.Btn`
- ஐகான் பொத்தான் `.BtnC`
- முதன்மை ஹைலைட் நிலை`.Btn.Main`

## பயன்படுத்த

உறுப்புக்கு வகுப்பின் பெயரைச் சேர்க்கவும்:

```html
<!-- இயல்பான பொத்தான் -->
<button class="Btn">உறுதி</button>

<!-- முக்கிய சிறப்பம்சமாக -->
<button class="Btn Main">சமர்ப்பிக்க</button>

<!-- இணைப்பு பொத்தான் -->
<a class="Btn" href="https://github.com">இணைப்பு பொத்தான்</a>
```

## ஐகான் பாணி

`BtnC`க்கான ஐகானை `background-image` மூலம் தனிப்பயனாக்கலாம்:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```