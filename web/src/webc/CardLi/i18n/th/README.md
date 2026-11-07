# โหลดและแสดงรายการการ์ดแบบอะซิงโครนัส

- **การโหลดแบบอะซิงโครนัส**: รับข้อมูลผ่านฟังก์ชันอะซิงโครนัส
- **การแสดงสถานะ**: รองรับสามสถานะ: กำลังโหลด ข้อมูลว่าง และการ์ดแสดงผล
- **รูปแบบที่ยืดหยุ่น**: การ์ดจะถูกจัดเรียงเป็นแถว

## ใช้การสาธิต

```html
<script type="module">
import CardLi from "webc.site/CardLi.js";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const card_li = CardLi(async () => {
  await sleep(2000);
  const data = [];
  for (let i = 1; i <= 13; ++i) {
    data.push(["หัวข้อ " + i, "คำอธิบาย" + i, "#" + i]);
  }
  return data;
});

document.body.append(card_li);
</script>
```

## พารามิเตอร์อินเทอร์เฟซ

### CardLi(loader)

- **ตัวโหลด**: `() => Promise<Array<[string, string, string]>>` ฟังก์ชันการโหลดข้อมูลแบบอะซิงโครนัส รูปแบบของแต่ละคีย์ย่อยคือ `[ชื่อ, บทนำ, ลิงก์]`

## คลาสสไตล์

- `.CardLi`: องค์ประกอบภายนอก
- `.Card`: องค์ประกอบการ์ด
- `.Ing`: กำลังโหลดองค์ประกอบภาพเคลื่อนไหว