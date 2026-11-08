# Սովորական կոճակ և կլոր պատկերակի կոճակ

## Դասի անվանումը

- Նորմալ կոճակ`.Btn`
- պատկերակի կոճակը `.BtnC`
- Հիմնական ընդգծված վիճակ`.Btn.Main`

## օգտագործել

Ավելացրեք դասի անուն տարրին՝

```html
<!-- Նորմալ կոճակ -->
<button class="Btn">հաստատել</button>

<!-- հիմնական շեշտադրումը -->
<button class="Btn Main">ներկայացնել</button>

<!-- հղում կոճակը -->
<a class="Btn" href="https://github.com">հղում կոճակը</a>
```

## պատկերակի ոճը

`BtnC`-ի պատկերակը կարող է հարմարեցվել `background-image`-ի միջոցով.

```html
<style>
.BtnC.add .Ico {
  background-image: url("data:image/svg+xml,<svg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 10 10' stroke='%23000'><path d='M5 1v8M1 5h8'/></svg>");
}
</style>
<button class="BtnC add"><i class="Ico"></i></button>
```