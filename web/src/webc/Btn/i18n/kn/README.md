# ಸಾಮಾನ್ಯ ಬಟನ್ ಮತ್ತು ಸುತ್ತಿನ ಐಕಾನ್ ಬಟನ್

## ವರ್ಗದ ಹೆಸರು

- ಸಾಮಾನ್ಯ ಬಟನ್`.Btn`
- ಐಕಾನ್ ಬಟನ್ `.BtnC`
- ಪ್ರಾಥಮಿಕ ಹೈಲೈಟ್ ಸ್ಥಿತಿ`.Btn.Main`

## ಬಳಸಿ

ಅಂಶಕ್ಕೆ ವರ್ಗದ ಹೆಸರನ್ನು ಸೇರಿಸಿ:

```html
<!-- ಸಾಮಾನ್ಯ ಬಟನ್ -->
<button class="Btn">ದೃಢೀಕರಿಸಿ</button>

<!-- ಮುಖ್ಯ ಹೈಲೈಟ್ -->
<button class="Btn Main">ಸಲ್ಲಿಸಿ</button>

<!-- ಲಿಂಕ್ ಬಟನ್ -->
<a class="Btn" href="https://github.com">ಲಿಂಕ್ ಬಟನ್</a>
```

## ಐಕಾನ್ ಶೈಲಿ

`BtnC` ಗಾಗಿ ಐಕಾನ್ ಅನ್ನು `background-image` ಮೂಲಕ ಕಸ್ಟಮೈಸ್ ಮಾಡಬಹುದು:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```