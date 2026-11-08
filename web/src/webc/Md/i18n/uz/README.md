# Markdown matnini HTMLga xavfsiz tahlil qiling va ko'rsating.

- **Xavfsiz sanitarizatsiya**: mahalliy `setHTML` API yoki `DOMParser` tozalagichlar yordamida XSS hujumlaridan himoya qiling
- **Kontentni chiqarish**: ishga tushirish vaqtida standart slotning matn tarkibini avtomatik o‘qing va uni ko‘rsating
- **O'rnatilgan matn terish**: oldindan o'rnatilgan ro'yxat, jadval, kod bloki, tirnoq va ogohlantirish qutisi uslublari

## Ko‘rsatma ishlatish

### statik renderlash

```html
<c-md>
# sarlavha
Bu Markdown tarkibining bir qismi
</c-md>
```

### dinamik topshiriq

```html
<c-md id="md"></c-md>

<script type="module">
import "webc.site/Md.js";
document.querySelector("#md").value = "#Yangi sarlavha\nYangi tarkib";
</script>
```

## Interfeys parametrlari

Markdown matnini toʻgʻridan-toʻgʻri teg ichida (standart slot sifatida) kiriting yoki `value` orqali dinamik yangilang.

## Uslub tavsifi

Komponent ishga tushirilganda `.Md` uslublar sinfi avtomatik ravishda qo'shiladi. Quyidagi Markdown kengaytirilgan ogohlantirish oynasi tartibini qo'llab-quvvatlaydi (ma'lumotnoma sintaksisi bilan ishga tushiriladi):

- `[!NOTE]`: Tezkor ma'lumot
- `[!TIP]`: Takliflar/Maslahatlar
- `[!IMPORTANT]`: Muhim e'tibor
- `[!WARNING]`: Xavf haqida ogohlantirish
- `[!CAUTION]`: Juda yuqori xavf haqida ogohlantirish

## xavfsizlik mexanizmi

Renderlashda HTMLni xavfsiz kiritish uchun avval mahalliy `setHTML` API-ga qo'ng'iroq qiling. Agar brauzer uni qo'llab-quvvatlamasa, u avtomatik ravishda `DOMParser` asosidagi rekursiv tozalagichga o'tadi va quyidagi xavfsizlik siyosatini amalga oshiradi:
- Xavfsiz bo'lmagan teglarni olib tashlang (masalan, `script`, `iframe` va hokazo.)
- `on*` hodisa atributlarini tozalang
- `javascript:`, `vbscript:` va `data:` (`data:image/`dan tashqari) kabi xavfli protokollardan boshlanadigan atributlarni filtrlash orqali XSS hujumlaridan himoya qiling.