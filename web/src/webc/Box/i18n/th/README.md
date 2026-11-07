# สร้างและแสดงเลเยอร์ป๊อปอัป

- การเรียกใช้ฟังก์ชันเพื่อสร้างและแสดงเลเยอร์ป๊อปอัป
- ลบองค์ประกอบออกจาก DOM โดยอัตโนมัติเมื่อปิด
- ป้องกันพฤติกรรมการยกเลิกคีย์ Esc

## ใช้การสาธิต

```html
<button>คลิกเพื่อป๊อปอัป</button>

<script type="module">
import Box from "webc.site/Box.js";

document.querySelector("button").onclick = () => {
  const dialog = Box(),
    content = document.createElement("div"),
    title = document.createElement("h3"),
    description = document.createElement("p"),
    button = document.createElement("button");

  content.className = "Lg";
  title.textContent = "ชื่อเรื่อง";
  description.textContent = "ข้อความ";
  button.className = "Btn Main";
  button.textContent = "ตกลง";
  button.onclick = () => dialog.close();
  content.append(title, description, button);
  dialog.append(content);
};
</script>
```

## พารามิเตอร์อินเทอร์เฟซ

### ฟังก์ชันส่งออกเริ่มต้น

`Box()`

- **ค่าส่งคืน**: `HTMLDialogElement` องค์ประกอบกล่องโต้ตอบของคลาสสไตล์ `Box` ได้รับการเพิ่มแล้ว

## คลาสสไตล์

### `.Box`

ใช้กับองค์ประกอบ `dialog` เพื่อตั้งค่าการวางตำแหน่ง การจัดกึ่งกลาง และการเบลอมาสก์พื้นหลัง

### `.Lg`

คลาสสไตล์ที่ใช้กับเนื้อหาภายใน การตั้งค่าสีพื้นหลัง มุมโค้งมน เงาและเค้าโครงแนวตั้ง และการกำหนดเค้าโครงของ `h3` และ `p` ภายใน