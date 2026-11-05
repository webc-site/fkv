# การรวมกันของกล่องการเลือกแบบเลื่อนลงและปุ่ม

- ประกบกล่องตัวเลือกและปุ่มในแนวนอน
- บันทึกสถานะที่เลือกลงใน localStorage โดยอัตโนมัติ
- สกัดกั้นการคลิกปุ่มและส่งเหตุการณ์ที่กำหนดเอง

## ใช้การสาธิต

```html
<c-select-btn key="my-select-key" value="option1">
  <c-select>
    <option value="option1">ตัวเลือกที่ 1</option>
    <option value="option2">ตัวเลือกที่ 2</option>
  </c-select>
  <button>วิ่ง</button>
</c-select-btn>

<script type="module">
  import "webc.site/SelectBtn.js";
  const el = document.querySelector("c-select-btn");
  el.addEventListener("submit", (e) => {
    console.log(e.value);
  });
</script>
```

## คุณสมบัติ

- `key`: ชื่อคีย์ที่ใช้สำหรับการคงอยู่ของ localStorage
- `value`: ค่าที่เลือกในปัจจุบัน

## เหตุการณ์

- `submit`: ส่งเมื่อมีการคลิกปุ่ม `e.value` ดำเนินการตามค่าที่เลือกในปัจจุบัน