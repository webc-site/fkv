# สร้างและแสดงเลเยอร์ป๊อปอัปด้วยปุ่มปิด

- การเรียกใช้ฟังก์ชันเพื่อสร้างและแสดงเลเยอร์ป๊อปอัป
- ปุ่มปิดในตัว คลิกเพื่อปิดและลบองค์ประกอบออกจาก DOM
- กด Esc เพื่อเบลอกล่องอินพุตหรือปิดเลเยอร์ป๊อปอัป

## ใช้การสาธิต

```html
<button>คลิกเพื่อป๊อปอัป</button>

<script type="module">
import BoxX from "webc.site/BoxX.js";

document.querySelector("button").onclick = () => {
  const dialog = BoxX(),
    main = document.createElement("main"),
    h3 = document.createElement("h3");
  main.classList.add("Lg");
  h3.textContent = "ข้อความ";
  main.appendChild(h3);
  dialog.appendChild(main);
};
</script>
```


## พารามิเตอร์อินเทอร์เฟซ

ฟังก์ชันการส่งออกเริ่มต้นส่งคืน `HTMLDialogElement` และมีการเพิ่มคลาสสไตล์ `Box` และ `X` ให้กับองค์ประกอบกล่องโต้ตอบ

## คลาสสไตล์

### `.Box.X`

นำไปใช้กับ `dialog` องค์ประกอบระบุรูปแบบการโต้ตอบ

### `a.X`

นำไปใช้กับ `a` องค์ประกอบระบุสไตล์ปุ่มปิด