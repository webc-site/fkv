# Sahifani aylantirganda avtomatik ravishda yiqilib yoki kengaytiriladigan yuqori navigatsiya paneli

- **Avtomatik yashirish**: sahifani pastga aylantirganda avtomatik ravishda yashirish.
- **Avtomatik kengaytirish**: sahifa yuqoriga aylantirilganda yoki aylantirish paneli yo‘qolganda avtomatik ravishda ko‘rsatiladi.
- **Umumiylikni optimallashtirish**: aylantirishni bostirish uchun `requestAnimationFrame` dan foydalaning.
- **Resurs manbalari**: Komponent yo‘q qilinganda voqea tinglovchilari va kuzatuvchilari avtomatik ravishda o‘chiriladi.

## Ko‘rsatma ishlatish

```html
<main>
  <c-vs>
    <nav class="Nav">
      <c-nav>
        <a class="logo" href="#">LOGO</a>
      </c-nav>
      <c-nav>
        <a class="item" href="#">Havola</a>
      </c-nav>
    </nav>
    <div class="content">Navigatsiyani yashirish uchun pastga suring, ko‘rsatish uchun tepaga suring</div>
  </c-vs>
</main>
<style>
  main {
    position: relative;
    height: 300px;
    border: 1px solid #000;
  }
  nav.Nav > c-nav {
    background: #fffc;
    backdrop-filter: blur(4px);
    border-bottom: 1px solid #ccc;
  }
  .content {
    height: 200dvh;
    background: linear-gradient(to bottom, #fff, #ccc);
    padding: 80px 20px 20px;
    color: #666;
    text-align: center;
  }
</style>
```

## Foydalanish bo'yicha ko'rsatmalar

`<c-vs>` ning asosiy yoki avlod elementi sifatida ishlatilishi kerak.

## uslub holati

Komponent quyidagi sinf nomini `nav.Nav` ga almashtiradi:

- `D`: Holatni ko'rsatish (pastga yoki tepaga suring).
- `I`: Oʻtish holati yashirin.
- `H`: Yashirin holat.

## uslub o'zgaruvchilari

- `--top`: Komponent tomonidan avtomatik ravishda balandlikning salbiy qiymatiga yangilanadi, joylashishni aniqlash va o'tish uchun ishlatiladi.