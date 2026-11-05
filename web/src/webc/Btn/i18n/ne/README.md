# साधारण बटन र गोल आइकन बटन

## कक्षाको नाम

- सामान्य बटन`.Btn`
- आइकन बटन `.BtnC`
- प्राथमिक हाइलाइट अवस्था`.Btn.Main`

## प्रयोग गर्नुहोस्

तत्वमा कक्षाको नाम थप्नुहोस्:

```html
<!-- सामान्य बटन -->
<button class="Btn">पुष्टि गर्नुहोस्</button>

<!-- मुख्य हाइलाइट -->
<button class="Btn Main">पेश गर्नुहोस्</button>

<!-- लिङ्क बटन -->
<a class="Btn" href="https://github.com">लिङ्क बटन</a>
```

## प्रतिमा शैली

`BtnC` को लागि आइकन `background-image` मार्फत अनुकूलित गर्न सकिन्छ:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```