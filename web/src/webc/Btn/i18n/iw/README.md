# כפתור רגיל וכפתור אייקון עגול

## שם הכיתה

- כפתור רגיל`.Btn`
- לחצן סמל `.BtnC`
- מצב הדגשה ראשי`.Btn.Main`

## לְהִשְׁתַמֵשׁ

הוסף שם מחלקה לאלמנט:

```html
<!-- כפתור רגיל -->
<button class="Btn">לְאַשֵׁר</button>

<!-- גולת הכותרת העיקרית -->
<button class="Btn Main">לְהַגִישׁ</button>

<!-- כפתור קישור -->
<a class="Btn" href="https://github.com">כפתור קישור</a>
```

## סגנון סמל

ניתן להתאים אישית את הסמל של `BtnC` באמצעות `background-image`:

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```