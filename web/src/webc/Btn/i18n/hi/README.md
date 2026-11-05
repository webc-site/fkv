# साधारण बटन और गोल आइकन बटन

## कक्षा का नाम

- सामान्य बटन`.Btn`
- आइकन बटन `.BtnC`
- प्राथमिक हाइलाइट स्थिति`.Btn.Main`

## उपयोग

तत्व में एक वर्ग का नाम जोड़ें:

```html
<!-- सामान्य बटन -->
<button class="Btn">पुष्टि करना</button>

<!-- मुख्य आकर्षण -->
<button class="Btn Main">जमा करना</button>

<!-- लिंक बटन -->
<a class="Btn" href="https://github.com">लिंक बटन</a>
```

## आइकन शैली

`BtnC` के लिए आइकन को `background-image` के माध्यम से अनुकूलित किया जा सकता है:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```