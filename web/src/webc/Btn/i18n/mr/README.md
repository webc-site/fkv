# सामान्य बटण आणि गोल चिन्ह बटण

## वर्गाचे नाव

- सामान्य बटण`.Btn`
- चिन्ह बटण `.BtnC`
- प्राथमिक हायलाइट स्थिती`.Btn.Main`

## वापर

घटकामध्ये वर्गाचे नाव जोडा:

```html
<!-- सामान्य बटण -->
<button class="Btn">पुष्टी करा</button>

<!-- मुख्य हायलाइट -->
<button class="Btn Main">सबमिट करा</button>

<!-- लिंक बटण -->
<a class="Btn" href="https://github.com">लिंक बटण</a>
```

## चिन्ह शैली

`BtnC` साठीचे चिन्ह `background-image` द्वारे सानुकूलित केले जाऊ शकते:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```