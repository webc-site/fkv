# virtual aylantirish paneli

- **Ikki tomonlama aylantirish**: Vertikal (`c-vs`) va gorizontal (`c-hs`) aylantirishni ta'minlaydi.
- **Oʻlchamli javob**: Tarkib va ​​maydon oʻlchami oʻzgarishlarini kuzatib boring, aylantirish paneli oʻlchami va oʻrnini avtomatik yangilang va toʻlib-toshgan boʻlmaganda avtomatik ravishda yashiring.
- **Interaktiv qo'llab-quvvatlash**: g'ildirakni aylantirish, slayderni sudrab borish va bosish joyini aniqlashni qo'llab-quvvatlaydi.
- **Holatni yashirish**: slayderni kursor yoki aylanayotganda ko‘rsatish va kengaytirish, harakatsiz holatda esa avtomatik ravishda yashirish.

## vertikal aylantirish

```html
<div style="height:300px;border:1px solid #000">
  <c-vs>
    <b>1</b>
    <b>2</b>
    <b>3</b>
  </c-vs>
</div>
<style>b { display: flex; border: 2px solid #090; padding: 30px; margin: 30px; }</style>
```

## gorizontal aylantirish

```html
<div style="width:300px;border:1px solid #000">
  <c-hs>
    <b>1</b>
    <b>2</b>
    <b>3</b>
  </c-hs>
</div>
<style>b { display: flex; border: 2px solid #090; margin: 30px; padding: 30px; }</style>
```

## uslub qismlari

Shadow DOM ichki elementlari uslubini `::part()` orqali sozlang:

- `::part(scroll)`: Kontent maydonini aylantiring.
- `::part(bar)`: aylantirish paneli treki.
- `::part(si)`: aylantirish paneli slayderi.
- `::part(i)`: Ichki o'rash qatlami.
- `::part(drag)`: O'tkazish holatidagi aylantirish paneli treki.

## uslublar sinfi

- `.drag`: `body` elementga qo'llaniladi, bu uning sudrab o'tish holatida ekanligini ko'rsatadi.

## Maxsus xususiyatlar

- `--si-bg`: Slayder fon rangi.
- `--si-anim`: Slayder animatsiyasi.
- `--cursorScrollhSvg`: Sichqoncha kursori, gorizontal aylantirish paneli kursorga o'tkazilganda.
- `--cursorScrollvSvg`: sichqoncha kursori vertikal aylantirish paneli kursorga o'tkazilganda.